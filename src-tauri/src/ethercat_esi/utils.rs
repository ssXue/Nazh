//! ESI / ENI 共享工具函数。

use nazh_dsl_core::device::{SignalSource, SignalSpec, SignalType};
use quick_xml::events::{BytesEnd, BytesStart};

use super::types::{
    EniPreviousPort, EniSlave, EniTextTarget, EsiDevice, EsiPdo, EsiPdoEntry, PdoDirection,
    TextTarget,
};

pub(super) fn xml_root_name(xml: &str) -> Result<Option<String>, String> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(element) | Event::Empty(element)) => {
                return Ok(Some(start_name(&element)));
            }
            Ok(Event::Eof) => return Ok(None),
            Err(error) => return Err(format!("ESI XML 解析失败: {error}")),
            _ => {}
        }
    }
}

pub(super) fn start_name(element: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(element.local_name().as_ref()).into_owned()
}

pub(super) fn end_name(element: &BytesEnd<'_>) -> String {
    String::from_utf8_lossy(element.local_name().as_ref()).into_owned()
}

pub(super) fn attr_string(element: &BytesStart<'_>, key: &[u8]) -> Option<String> {
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

pub(super) fn attr_number(element: &BytesStart<'_>, key: &[u8]) -> Option<u32> {
    attr_string(element, key).and_then(|value| parse_esi_u32(&value))
}

pub(super) fn to_u16(value: u32) -> Option<u16> {
    u16::try_from(value).ok()
}

pub(super) fn to_u8(value: u32) -> Option<u8> {
    u8::try_from(value).ok()
}

pub(super) fn finish_entry(
    current_pdo: &mut Option<EsiPdo>,
    current_entry: &mut Option<EsiPdoEntry>,
) {
    if let (Some(pdo), Some(entry)) = (current_pdo.as_mut(), current_entry.take()) {
        pdo.entries.push(entry);
    }
}

/// 对字节序列做确定性哈希（FNV-1a 风格的 32 位折半）。
pub(super) fn deterministic_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in data {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// 从 `ProductCode` 派生 6 位数字设备 ID。
///
/// 例如 Beckhoff EL1008（`ProductCode` `0x03F03052`）→ `"195538"`。
/// 没有 `ProductCode` 时，使用设备标签的稳定哈希值。
pub(super) fn default_device_id(device: &EsiDevice) -> String {
    if let Some(product_code) = device.product_code {
        return format!("{:06}", product_code % 1_000_000);
    }
    // Fallback：没有 ProductCode（如 ENI 网络设备），对标签做确定性哈希
    let label = device
        .type_name
        .as_deref()
        .or(device.name.as_deref())
        .unwrap_or("ethercat_device");
    let hash = deterministic_hash(label.as_bytes());
    format!("{:06}", hash % 1_000_000)
}

pub(super) fn model_label(device: &EsiDevice) -> Option<String> {
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

pub(super) fn build_signals(device: &EsiDevice, warnings: &mut Vec<String>) -> Vec<SignalSpec> {
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
            if entry.index == Some(0) {
                continue;
            }
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
                    slave_address: pdo.slave_address,
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

pub(super) fn signal_type_for(
    direction: PdoDirection,
    bit_len: u16,
    data_type: Option<&str>,
) -> SignalType {
    let digital =
        bit_len == 1 || data_type.is_some_and(|value| value.to_ascii_uppercase().contains("BOOL"));
    match (direction, digital) {
        (PdoDirection::Tx, true) => SignalType::DigitalInput,
        (PdoDirection::Tx, false) => SignalType::AnalogInput,
        (PdoDirection::Rx, true) => SignalType::DigitalOutput,
        (PdoDirection::Rx, false) => SignalType::AnalogOutput,
    }
}

pub(super) fn unique_signal_id(
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
    let base = pdo.signal_prefix.as_deref().map_or(base.clone(), |prefix| {
        if prefix.is_empty() {
            base.clone()
        } else {
            format!("{prefix}_{base}")
        }
    });
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

pub(super) fn sanitize_identifier(value: &str) -> String {
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

pub(super) fn parse_esi_u32(value: &str) -> Option<u32> {
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

pub(super) fn has_parent(path: &[String], name: &str) -> bool {
    path.iter()
        .rev()
        .skip(1)
        .any(|segment| segment.as_str() == name)
}

pub(super) fn is_config_slave_path(path: &[String]) -> bool {
    path.last().is_some_and(|segment| segment == "Slave")
        && path.iter().any(|segment| segment == "Config")
}

pub(super) fn is_devices_device_path(path: &[String]) -> bool {
    path.last().is_some_and(|segment| segment == "Device")
        && path.iter().any(|segment| segment == "Devices")
}

pub(super) fn classify_text_target(path: &[String], name: &str) -> Option<TextTarget> {
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

pub(super) fn apply_text_target(
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

pub(super) fn classify_eni_text_target(path: &[String], name: &str) -> Option<EniTextTarget> {
    match name {
        "Name"
            if has_parent(path, "Info")
                && has_parent(path, "Slave")
                && !has_parent(path, "TxPdo")
                && !has_parent(path, "RxPdo") =>
        {
            Some(EniTextTarget::SlaveName)
        }
        "ProductRevision" if has_parent(path, "Info") && has_parent(path, "Slave") => {
            Some(EniTextTarget::ProductRevision)
        }
        "ProductCode" if has_parent(path, "Info") && has_parent(path, "Slave") => {
            Some(EniTextTarget::ProductCode)
        }
        "RevisionNo" if has_parent(path, "Info") && has_parent(path, "Slave") => {
            Some(EniTextTarget::RevisionNo)
        }
        "PhysAddr" if has_parent(path, "PreviousPort") => Some(EniTextTarget::PreviousPhysAddr),
        "PhysAddr" if has_parent(path, "Info") && has_parent(path, "Slave") => {
            Some(EniTextTarget::PhysAddr)
        }
        "Port" if has_parent(path, "PreviousPort") => Some(EniTextTarget::PreviousPort),
        "Pdo" if has_parent(path, "Sm2") => Some(EniTextTarget::Sm2Pdo),
        "Pdo" if has_parent(path, "Sm3") => Some(EniTextTarget::Sm3Pdo),
        "Index" if has_parent(path, "Entry") => Some(EniTextTarget::EntryIndex),
        "SubIndex" if has_parent(path, "Entry") => Some(EniTextTarget::EntrySubIndex),
        "BitLen" if has_parent(path, "Entry") => Some(EniTextTarget::EntryBitLen),
        "Name" if has_parent(path, "Entry") => Some(EniTextTarget::EntryName),
        "DataType" if has_parent(path, "Entry") => Some(EniTextTarget::EntryDataType),
        "Index" if has_parent(path, "TxPdo") || has_parent(path, "RxPdo") => {
            Some(EniTextTarget::PdoIndex)
        }
        "Name" if has_parent(path, "TxPdo") || has_parent(path, "RxPdo") => {
            Some(EniTextTarget::PdoName)
        }
        _ => None,
    }
}

pub(super) fn apply_eni_text_target(
    target: EniTextTarget,
    value: &str,
    current_slave: &mut Option<EniSlave>,
    current_pdo: &mut Option<EsiPdo>,
    current_entry: &mut Option<EsiPdoEntry>,
) {
    if value.is_empty() {
        return;
    }

    match target {
        EniTextTarget::SlaveName => {
            if let Some(slave) = current_slave.as_mut() {
                slave.name = Some(value.to_owned());
            }
        }
        EniTextTarget::ProductRevision => {
            if let Some(slave) = current_slave.as_mut() {
                slave.product_revision = Some(value.to_owned());
            }
        }
        EniTextTarget::ProductCode => {
            if let Some(slave) = current_slave.as_mut() {
                slave.product_code = parse_esi_u32(value);
            }
        }
        EniTextTarget::RevisionNo => {
            if let Some(slave) = current_slave.as_mut() {
                slave.revision_no = parse_esi_u32(value);
            }
        }
        EniTextTarget::PhysAddr => {
            if let Some(slave) = current_slave.as_mut() {
                slave.phys_addr = parse_esi_u32(value).and_then(to_u16);
            }
        }
        EniTextTarget::PreviousPort => {
            if let Some(slave) = current_slave.as_mut() {
                slave
                    .previous_port
                    .get_or_insert_with(EniPreviousPort::default)
                    .port = Some(value.to_owned());
            }
        }
        EniTextTarget::PreviousPhysAddr => {
            if let Some(slave) = current_slave.as_mut() {
                slave
                    .previous_port
                    .get_or_insert_with(EniPreviousPort::default)
                    .phys_addr = parse_esi_u32(value).and_then(to_u16);
            }
        }
        EniTextTarget::Sm2Pdo => {
            if let (Some(slave), Some(value)) = (
                current_slave.as_mut(),
                parse_esi_u32(value).and_then(to_u16),
            ) {
                push_unique(&mut slave.active_rx_pdos, value);
            }
        }
        EniTextTarget::Sm3Pdo => {
            if let (Some(slave), Some(value)) = (
                current_slave.as_mut(),
                parse_esi_u32(value).and_then(to_u16),
            ) {
                push_unique(&mut slave.active_tx_pdos, value);
            }
        }
        EniTextTarget::PdoIndex => {
            if let Some(pdo) = current_pdo.as_mut() {
                pdo.index = parse_esi_u32(value).and_then(to_u16);
            }
        }
        EniTextTarget::PdoName => {
            if let Some(pdo) = current_pdo.as_mut() {
                pdo.name = Some(value.to_owned());
            }
        }
        EniTextTarget::EntryIndex => {
            if let Some(entry) = current_entry.as_mut() {
                entry.index = parse_esi_u32(value).and_then(to_u16);
            }
        }
        EniTextTarget::EntrySubIndex => {
            if let Some(entry) = current_entry.as_mut() {
                entry.sub_index = parse_esi_u32(value).and_then(to_u8);
            }
        }
        EniTextTarget::EntryBitLen => {
            if let Some(entry) = current_entry.as_mut() {
                entry.bit_len = parse_esi_u32(value).and_then(to_u16);
            }
        }
        EniTextTarget::EntryName => {
            if let Some(entry) = current_entry.as_mut() {
                entry.name = Some(value.to_owned());
            }
        }
        EniTextTarget::EntryDataType => {
            if let Some(entry) = current_entry.as_mut() {
                entry.data_type = Some(value.to_owned());
            }
        }
    }
}

pub(super) fn push_unique(values: &mut Vec<u16>, value: u16) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use nazh_dsl_core::device::{SignalSource, SignalType};
    use nazh_dsl_core::parse_device_yaml;

    use crate::ethercat_esi::esi_import::import_esi_to_device_yaml;

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

    const SAMPLE_ENI: &str = r#"
<EtherCATConfig Version="1.3">
  <Config>
    <Slave>
      <Info>
        <Name><![CDATA[Box 1 (CU1128)]]></Name>
        <PhysAddr>1001</PhysAddr>
        <ProductCode>73946162</ProductCode>
        <RevisionNo>131072</RevisionNo>
      </Info>
      <ProcessData/>
    </Slave>
    <Slave>
      <Info>
        <Name><![CDATA[Drive 2 (Elmo Drive )]]></Name>
        <PhysAddr>1002</PhysAddr>
        <ProductCode>198948</ProductCode>
        <RevisionNo>66592</RevisionNo>
      </Info>
      <ProcessData>
        <Sm2>
          <Type>Outputs</Type>
          <Pdo>5637</Pdo>
        </Sm2>
        <Sm3>
          <Type>Inputs</Type>
          <Pdo>6658</Pdo>
        </Sm3>
        <TxPdo Fixed="true">
          <Index>#x1a02</Index>
          <Name>Inputs</Name>
          <Entry>
            <Index>#x6064</Index>
            <SubIndex>0</SubIndex>
            <BitLen>32</BitLen>
            <Name>Position actual value</Name>
            <DataType>DINT</DataType>
          </Entry>
          <Entry>
            <Index>#x0</Index>
            <BitLen>8</BitLen>
          </Entry>
        </TxPdo>
        <RxPdo Fixed="true">
          <Index>#x1605</Index>
          <Name>Outputs</Name>
          <Entry>
            <Index>#x607a</Index>
            <SubIndex>0</SubIndex>
            <BitLen>32</BitLen>
            <Name>Target Position</Name>
            <DataType>DINT</DataType>
          </Entry>
        </RxPdo>
        <RxPdo Fixed="true">
          <Index>#x1606</Index>
          <Name>Inactive Outputs</Name>
          <Entry>
            <Index>#x6040</Index>
            <SubIndex>0</SubIndex>
            <BitLen>16</BitLen>
            <Name>Control word</Name>
            <DataType>UINT</DataType>
          </Entry>
        </RxPdo>
      </ProcessData>
      <PreviousPort Selected="true">
        <Port>D</Port>
        <PhysAddr>1001</PhysAddr>
      </PreviousPort>
    </Slave>
  </Config>
</EtherCATConfig>
"#;

    #[test]
    fn esi_导入为_device_yaml() {
        let result = import_esi_to_device_yaml(SAMPLE_ESI).unwrap();
        assert_eq!(result.device_yamls.len(), 1);
        let spec = parse_device_yaml(&result.device_yamls[0]).unwrap();
        assert_eq!(spec.id, "072658");
        assert_eq!(spec.manufacturer.as_deref(), Some("Beckhoff"));
        assert!(spec.connection.is_none());
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
    fn esi_导入不生成_connection() {
        let result = import_esi_to_device_yaml(SAMPLE_ESI).unwrap();
        let spec = parse_device_yaml(&result.device_yamls[0]).unwrap();
        assert_eq!(spec.id, "072658");
        assert!(!result.device_yamls[0].contains("connection:"));
    }

    #[test]
    fn esi_导入默认不生成_connection() {
        let result = import_esi_to_device_yaml(SAMPLE_ESI).unwrap();
        assert!(!result.device_yamls[0].contains("connection:"));
    }

    #[test]
    fn eni_每从站独立设备且共享组() {
        let result = import_esi_to_device_yaml(SAMPLE_ENI).unwrap();

        // ENI 中只有 Slave 1002 有激活 PDO，Slave 1001 无 ProcessData
        assert_eq!(result.device_yamls.len(), 1);
        assert!(
            result
                .warnings
                .iter()
                .any(|item| item.contains("EtherCATConfig/ENI"))
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|item| item.contains("1002 Drive 2"))
        );

        let spec = parse_device_yaml(&result.device_yamls[0]).unwrap();
        assert_eq!(spec.device_type, "ethercat_slave");
        assert_eq!(spec.signals.len(), 2);
        assert!(spec.network_group.is_some());
        assert!(spec.network_group.as_deref().unwrap().starts_with("eni_"));

        // 信号不再有从站前缀（因为每个从站是独立设备）
        assert!(spec.signals.iter().any(|signal| {
            signal.id == "position_actual_value"
                && matches!(
                    signal.source,
                    SignalSource::EthercatPdo {
                        slave_address: Some(1002),
                        pdo_index: 0x1A02,
                        entry_index: 0x6064,
                        ..
                    }
                )
        }));
        assert!(spec.signals.iter().any(|signal| {
            signal.id == "target_position"
                && matches!(
                    signal.source,
                    SignalSource::EthercatPdo {
                        slave_address: Some(1002),
                        pdo_index: 0x1605,
                        entry_index: 0x607A,
                        ..
                    }
                )
        }));
        assert!(
            !result.device_yamls[0].contains("control_word"),
            "未激活 PDO 不应导入"
        );
    }
}
