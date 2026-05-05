//! `EtherCAT` ESI 文件导入器。
//!
//! ESI（EtherCAT Slave Information）是设备厂商发布的 XML 描述文件。
//! 本模块只做确定性解析：提取厂商、型号、ProductCode、RevisionNo 与 PDO 条目，
//! 再转换为 Device DSL YAML，供用户审查后保存为设备资产。

use nazh_dsl_core::device::{ConnectionRef, DeviceSpec, SignalSource, SignalSpec, SignalType};
use quick_xml::Reader;
use quick_xml::events::{BytesEnd, BytesStart, Event};

/// ESI 导入结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EsiImportResult {
    pub(crate) device_yaml: String,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct EsiDevice {
    vendor_name: Option<String>,
    name: Option<String>,
    type_name: Option<String>,
    product_code: Option<u32>,
    revision_no: Option<u32>,
    tx_pdos: Vec<EsiPdo>,
    rx_pdos: Vec<EsiPdo>,
}

#[derive(Debug, Clone)]
struct EsiPdo {
    direction: PdoDirection,
    index: Option<u16>,
    name: Option<String>,
    entries: Vec<EsiPdoEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PdoDirection {
    /// TxPDO：从站发给主站，建模为设备输入信号。
    Tx,
    /// RxPDO：主站写给从站，建模为设备输出信号。
    Rx,
}

#[derive(Debug, Clone, Default)]
struct EsiPdoEntry {
    index: Option<u16>,
    sub_index: Option<u8>,
    bit_len: Option<u16>,
    name: Option<String>,
    data_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextTarget {
    VendorName,
    DeviceName,
    DeviceType,
    PdoIndex,
    PdoName,
    EntryIndex,
    EntrySubIndex,
    EntryBitLen,
    EntryName,
    EntryDataType,
}

/// 将 ESI XML 转换为 Device DSL YAML。
pub(crate) fn import_esi_to_device_yaml(
    esi_xml: &str,
    connection_id: Option<&str>,
    requested_device_id: Option<&str>,
) -> Result<EsiImportResult, String> {
    let devices = parse_esi_devices(esi_xml)?;
    let Some(device) = devices.first() else {
        return Err("ESI 文件中没有找到 Descriptions/Devices/Device".to_owned());
    };

    let mut warnings = Vec::new();
    if devices.len() > 1 {
        warnings.push(format!(
            "ESI 文件包含 {} 个 Device，本次导入第一个设备；如需其它型号请拆分 ESI 后再导入",
            devices.len()
        ));
    }

    let device_id = requested_device_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| default_device_id(device), sanitize_identifier);
    let connection_id = connection_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("ethercat_main")
        .to_owned();
    if connection_id == "ethercat_main" {
        warnings.push(
            "connection.id 使用占位值 ethercat_main，保存前请改为连接资源里的真实 ID".to_owned(),
        );
    }

    let signals = build_signals(device, &mut warnings);
    if signals.is_empty() {
        warnings.push("未从 ESI 中解析到 PDO Entry，设备 YAML 只包含基本型号信息".to_owned());
    }

    let spec = DeviceSpec {
        id: device_id,
        device_type: "ethercat_slave".to_owned(),
        manufacturer: device.vendor_name.clone(),
        model: model_label(device),
        connection: ConnectionRef {
            connection_type: "ethercat".to_owned(),
            id: connection_id,
            unit: None,
        },
        signals,
        alarms: Vec::new(),
    };

    let device_yaml = serde_yaml::to_string(&spec)
        .map_err(|error| format!("DeviceSpec YAML 序列化失败: {error}"))?;
    Ok(EsiImportResult {
        device_yaml,
        warnings,
    })
}

#[allow(clippy::too_many_lines)]
fn parse_esi_devices(esi_xml: &str) -> Result<Vec<EsiDevice>, String> {
    let mut reader = Reader::from_str(esi_xml);
    reader.config_mut().trim_text(true);

    let mut path = Vec::<String>::new();
    let mut text = String::new();
    let mut devices = Vec::new();
    let mut vendor_name = None;
    let mut current_device: Option<EsiDevice> = None;
    let mut current_pdo: Option<EsiPdo> = None;
    let mut current_entry: Option<EsiPdoEntry> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                let name = start_name(&element);
                path.push(name.clone());
                text.clear();
                match name.as_str() {
                    "Device" if is_devices_device_path(&path) => {
                        current_device = Some(EsiDevice {
                            vendor_name: vendor_name.clone(),
                            ..EsiDevice::default()
                        });
                    }
                    "Type" if current_device.is_some() && current_pdo.is_none() => {
                        if let Some(device) = current_device.as_mut() {
                            device.product_code = attr_number(&element, b"ProductCode");
                            device.revision_no = attr_number(&element, b"RevisionNo");
                        }
                    }
                    "TxPdo" if current_device.is_some() => {
                        current_pdo = Some(EsiPdo {
                            direction: PdoDirection::Tx,
                            index: None,
                            name: None,
                            entries: Vec::new(),
                        });
                    }
                    "RxPdo" if current_device.is_some() => {
                        current_pdo = Some(EsiPdo {
                            direction: PdoDirection::Rx,
                            index: None,
                            name: None,
                            entries: Vec::new(),
                        });
                    }
                    "Entry" if current_pdo.is_some() => {
                        current_entry = Some(EsiPdoEntry::default());
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(element)) => {
                let name = start_name(&element);
                if name == "Entry" && current_pdo.is_some() {
                    current_entry = Some(EsiPdoEntry {
                        index: attr_number(&element, b"Index").and_then(to_u16),
                        sub_index: attr_number(&element, b"SubIndex").and_then(to_u8),
                        bit_len: attr_number(&element, b"BitLen").and_then(to_u16),
                        name: attr_string(&element, b"Name"),
                        data_type: attr_string(&element, b"DataType"),
                    });
                    finish_entry(&mut current_pdo, &mut current_entry);
                }
            }
            Ok(Event::Text(event)) => {
                let decoded = event
                    .decode()
                    .map_err(|error| format!("ESI 文本解码失败: {error}"))?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map_err(|error| format!("ESI 文本转义解析失败: {error}"))?;
                text.push_str(&unescaped);
            }
            Ok(Event::CData(event)) => {
                let decoded = event
                    .decode()
                    .map_err(|error| format!("ESI CDATA 解码失败: {error}"))?;
                text.push_str(&decoded);
            }
            Ok(Event::End(element)) => {
                let name = end_name(&element);
                if let Some(target) = classify_text_target(&path, &name) {
                    apply_text_target(
                        target,
                        text.trim(),
                        &mut vendor_name,
                        &mut current_device,
                        &mut current_pdo,
                        &mut current_entry,
                    );
                }
                match name.as_str() {
                    "Entry" => finish_entry(&mut current_pdo, &mut current_entry),
                    "TxPdo" | "RxPdo" => {
                        if let (Some(device), Some(pdo)) =
                            (current_device.as_mut(), current_pdo.take())
                        {
                            match pdo.direction {
                                PdoDirection::Tx => device.tx_pdos.push(pdo),
                                PdoDirection::Rx => device.rx_pdos.push(pdo),
                            }
                        }
                    }
                    "Device" if current_device.is_some() && is_devices_device_path(&path) => {
                        if let Some(device) = current_device.take() {
                            devices.push(device);
                        }
                    }
                    _ => {}
                }
                let _ = path.pop();
                text.clear();
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("ESI XML 解析失败: {error}")),
            _ => {}
        }
    }

    Ok(devices)
}

fn classify_text_target(path: &[String], name: &str) -> Option<TextTarget> {
    match name {
        "Name" if has_parent(path, "Vendor") => Some(TextTarget::VendorName),
        "Name"
            if has_parent(path, "Device")
                && !has_parent(path, "TxPdo")
                && !has_parent(path, "RxPdo") =>
        {
            Some(TextTarget::DeviceName)
        }
        "Type"
            if has_parent(path, "Device")
                && !has_parent(path, "TxPdo")
                && !has_parent(path, "RxPdo") =>
        {
            Some(TextTarget::DeviceType)
        }
        "Index" if has_parent(path, "Entry") => Some(TextTarget::EntryIndex),
        "SubIndex" if has_parent(path, "Entry") => Some(TextTarget::EntrySubIndex),
        "BitLen" if has_parent(path, "Entry") => Some(TextTarget::EntryBitLen),
        "Name" if has_parent(path, "Entry") => Some(TextTarget::EntryName),
        "DataType" if has_parent(path, "Entry") => Some(TextTarget::EntryDataType),
        "Index" if has_parent(path, "TxPdo") || has_parent(path, "RxPdo") => {
            Some(TextTarget::PdoIndex)
        }
        "Name" if has_parent(path, "TxPdo") || has_parent(path, "RxPdo") => {
            Some(TextTarget::PdoName)
        }
        _ => None,
    }
}

fn apply_text_target(
    target: TextTarget,
    value: &str,
    vendor_name: &mut Option<String>,
    current_device: &mut Option<EsiDevice>,
    current_pdo: &mut Option<EsiPdo>,
    current_entry: &mut Option<EsiPdoEntry>,
) {
    if value.is_empty() {
        return;
    }

    match target {
        TextTarget::VendorName => *vendor_name = Some(value.to_owned()),
        TextTarget::DeviceName => {
            if let Some(device) = current_device.as_mut() {
                device.name = Some(value.to_owned());
            }
        }
        TextTarget::DeviceType => {
            if let Some(device) = current_device.as_mut() {
                device.type_name = Some(value.to_owned());
            }
        }
        TextTarget::PdoIndex => {
            if let Some(pdo) = current_pdo.as_mut() {
                pdo.index = parse_esi_u32(value).and_then(to_u16);
            }
        }
        TextTarget::PdoName => {
            if let Some(pdo) = current_pdo.as_mut() {
                pdo.name = Some(value.to_owned());
            }
        }
        TextTarget::EntryIndex => {
            if let Some(entry) = current_entry.as_mut() {
                entry.index = parse_esi_u32(value).and_then(to_u16);
            }
        }
        TextTarget::EntrySubIndex => {
            if let Some(entry) = current_entry.as_mut() {
                entry.sub_index = parse_esi_u32(value).and_then(to_u8);
            }
        }
        TextTarget::EntryBitLen => {
            if let Some(entry) = current_entry.as_mut() {
                entry.bit_len = parse_esi_u32(value).and_then(to_u16);
            }
        }
        TextTarget::EntryName => {
            if let Some(entry) = current_entry.as_mut() {
                entry.name = Some(value.to_owned());
            }
        }
        TextTarget::EntryDataType => {
            if let Some(entry) = current_entry.as_mut() {
                entry.data_type = Some(value.to_owned());
            }
        }
    }
}

fn finish_entry(current_pdo: &mut Option<EsiPdo>, current_entry: &mut Option<EsiPdoEntry>) {
    if let (Some(pdo), Some(entry)) = (current_pdo.as_mut(), current_entry.take()) {
        pdo.entries.push(entry);
    }
}

fn build_signals(device: &EsiDevice, warnings: &mut Vec<String>) -> Vec<SignalSpec> {
    let mut signals = Vec::new();
    for pdo in device.tx_pdos.iter().chain(device.rx_pdos.iter()) {
        let Some(pdo_index) = pdo.index else {
            warnings.push(format!(
                "{} 缺少 PDO Index，已跳过其中 {} 个 Entry",
                pdo.name.as_deref().unwrap_or("未命名 PDO"),
                pdo.entries.len()
            ));
            continue;
        };
        for (entry_idx, entry) in pdo.entries.iter().enumerate() {
            let Some(entry_index) = entry.index else {
                warnings.push(format!(
                    "PDO 0x{pdo_index:04X} 的第 {} 个 Entry 缺少 Index，已跳过",
                    entry_idx + 1
                ));
                continue;
            };
            let Some(sub_index) = entry.sub_index else {
                warnings.push(format!(
                    "PDO 0x{pdo_index:04X} Entry 0x{entry_index:04X} 缺少 SubIndex，已跳过"
                ));
                continue;
            };
            let bit_len = entry.bit_len.unwrap_or_else(|| {
                warnings.push(format!(
                    "PDO 0x{pdo_index:04X} Entry 0x{entry_index:04X}:{sub_index} 缺少 BitLen，默认按 1 bit 导入"
                ));
                1
            });
            let signal_type = signal_type_for(pdo.direction, bit_len, entry.data_type.as_deref());
            let id = unique_signal_id(&signals, pdo, entry, entry_index, sub_index);
            signals.push(SignalSpec {
                id,
                signal_type,
                unit: None,
                range: None,
                source: SignalSource::EthercatPdo {
                    pdo_index,
                    entry_index,
                    sub_index,
                    bit_len,
                    data_type: entry.data_type.clone(),
                    pdo_name: pdo.name.clone(),
                    entry_name: entry.name.clone(),
                },
                scale: None,
            });
        }
    }
    signals
}

fn signal_type_for(direction: PdoDirection, bit_len: u16, data_type: Option<&str>) -> SignalType {
    let digital =
        bit_len == 1 || data_type.is_some_and(|value| value.to_ascii_uppercase().contains("BOOL"));
    match (direction, digital) {
        (PdoDirection::Tx, true) => SignalType::DigitalInput,
        (PdoDirection::Tx, false) => SignalType::AnalogInput,
        (PdoDirection::Rx, true) => SignalType::DigitalOutput,
        (PdoDirection::Rx, false) => SignalType::AnalogOutput,
    }
}

fn unique_signal_id(
    existing: &[SignalSpec],
    pdo: &EsiPdo,
    entry: &EsiPdoEntry,
    entry_index: u16,
    sub_index: u8,
) -> String {
    let direction = match pdo.direction {
        PdoDirection::Tx => "tx",
        PdoDirection::Rx => "rx",
    };
    let base = entry
        .name
        .as_deref()
        .map(sanitize_identifier)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("{direction}_{entry_index:04x}_{sub_index}"));
    if !existing.iter().any(|signal| signal.id == base) {
        return base;
    }
    let mut seq = 2usize;
    loop {
        let candidate = format!("{base}_{seq}");
        if !existing.iter().any(|signal| signal.id == candidate) {
            return candidate;
        }
        seq += 1;
    }
}

fn default_device_id(device: &EsiDevice) -> String {
    let seed = device
        .type_name
        .as_deref()
        .or(device.name.as_deref())
        .map(sanitize_identifier)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if let Some(product_code) = device.product_code {
                format!("ethercat_{product_code:08x}")
            } else {
                "ethercat_device".to_owned()
            }
        });
    if seed
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic())
    {
        seed
    } else {
        format!("device_{seed}")
    }
}

fn model_label(device: &EsiDevice) -> Option<String> {
    let base = device.type_name.clone().or_else(|| device.name.clone())?;
    let mut parts = Vec::new();
    if let Some(product_code) = device.product_code {
        parts.push(format!("ProductCode 0x{product_code:08X}"));
    }
    if let Some(revision_no) = device.revision_no {
        parts.push(format!("Revision 0x{revision_no:08X}"));
    }
    if parts.is_empty() {
        Some(base)
    } else {
        Some(format!("{base} ({})", parts.join(", ")))
    }
}

fn sanitize_identifier(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_sep = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            output.push('_');
            last_was_sep = true;
        }
    }
    output.trim_matches('_').to_owned()
}

fn parse_esi_u32(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("#x")
        .or_else(|| trimmed.strip_prefix("#X"))
        .or_else(|| trimmed.strip_prefix("0x"))
        .or_else(|| trimmed.strip_prefix("0X"));
    if let Some(hex) = hex {
        return u32::from_str_radix(hex, 16).ok();
    }
    trimmed.parse::<u32>().ok()
}

fn to_u16(value: u32) -> Option<u16> {
    u16::try_from(value).ok()
}

fn to_u8(value: u32) -> Option<u8> {
    u8::try_from(value).ok()
}

fn start_name(element: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(element.local_name().as_ref()).into_owned()
}

fn end_name(element: &BytesEnd<'_>) -> String {
    String::from_utf8_lossy(element.local_name().as_ref()).into_owned()
}

fn attr_string(element: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    element
        .attributes()
        .with_checks(false)
        .flatten()
        .find(|attr| attr.key.local_name().as_ref() == key)
        .and_then(|attr| {
            attr.decode_and_unescape_value(element.decoder())
                .ok()
                .map(|value| value.trim().to_owned())
        })
        .filter(|value| !value.is_empty())
}

fn attr_number(element: &BytesStart<'_>, key: &[u8]) -> Option<u32> {
    attr_string(element, key).and_then(|value| parse_esi_u32(&value))
}

fn has_parent(path: &[String], name: &str) -> bool {
    path.iter()
        .rev()
        .skip(1)
        .any(|segment| segment.as_str() == name)
}

fn is_devices_device_path(path: &[String]) -> bool {
    path.last().is_some_and(|segment| segment == "Device")
        && path.iter().any(|segment| segment == "Devices")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nazh_dsl_core::parse_device_yaml;

    const SAMPLE_ESI: &str = r##"
<EtherCATInfo>
  <Vendor>
    <Id>#x00000002</Id>
    <Name>Beckhoff</Name>
  </Vendor>
  <Descriptions>
    <Devices>
      <Device>
        <Type ProductCode="#x03F03052" RevisionNo="#x00120000">EL1008</Type>
        <Name>8Ch. Dig. Input</Name>
        <RxPdo>
          <Index>#x1600</Index>
          <Name>Channel outputs</Name>
          <Entry>
            <Index>#x7000</Index>
            <SubIndex>1</SubIndex>
            <BitLen>1</BitLen>
            <Name>Output 1</Name>
            <DataType>BOOL</DataType>
          </Entry>
        </RxPdo>
        <TxPdo>
          <Index>#x1A00</Index>
          <Name>Channel inputs</Name>
          <Entry>
            <Index>#x6000</Index>
            <SubIndex>1</SubIndex>
            <BitLen>1</BitLen>
            <Name>Input 1</Name>
            <DataType>BOOL</DataType>
          </Entry>
        </TxPdo>
      </Device>
    </Devices>
  </Descriptions>
</EtherCATInfo>
"##;

    #[test]
    fn esi_导入为_device_yaml() {
        let result = import_esi_to_device_yaml(SAMPLE_ESI, Some("ecat_main"), None).unwrap();
        let spec = parse_device_yaml(&result.device_yaml).unwrap();
        assert_eq!(spec.id, "el1008");
        assert_eq!(spec.manufacturer.as_deref(), Some("Beckhoff"));
        assert_eq!(spec.connection.connection_type, "ethercat");
        assert_eq!(spec.connection.id, "ecat_main");
        assert_eq!(spec.signals.len(), 2);
        assert_eq!(spec.signals[0].signal_type, SignalType::DigitalInput);
        assert_eq!(spec.signals[1].signal_type, SignalType::DigitalOutput);
        assert!(matches!(
            spec.signals[0].source,
            SignalSource::EthercatPdo {
                pdo_index: 0x1A00,
                entry_index: 0x6000,
                sub_index: 1,
                ..
            }
        ));
    }

    #[test]
    fn esi_缺少连接时生成占位警告() {
        let result = import_esi_to_device_yaml(SAMPLE_ESI, None, Some("axis")).unwrap();
        assert!(result.device_yaml.contains("id: axis"));
        assert!(
            result
                .warnings
                .iter()
                .any(|item| item.contains("ethercat_main"))
        );
    }
}
