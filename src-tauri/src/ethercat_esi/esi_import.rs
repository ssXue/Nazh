//! ESI（EtherCAT Slave Information）解析逻辑。

use nazh_dsl_core::device::DeviceSpec;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::eni_import::import_eni_to_device_yaml;
use super::types::{EsiDevice, EsiImportResult, EsiPdo, EsiPdoEntry, PdoDirection};
use super::utils::{
    apply_text_target, attr_number, attr_string, build_signals, classify_text_target,
    default_device_id, end_name, finish_entry, is_devices_device_path, model_label, start_name,
    to_u8, to_u16, xml_root_name,
};

/// 将 ESI XML 转换为 Device DSL YAML。
pub(crate) fn import_esi_to_device_yaml(esi_xml: &str) -> Result<EsiImportResult, String> {
    if xml_root_name(esi_xml)? == Some("EtherCATConfig".to_owned()) {
        return import_eni_to_device_yaml(esi_xml);
    }

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

    build_device_yaml(device, &mut warnings)
}

fn build_device_yaml(
    device: &EsiDevice,
    warnings: &mut Vec<String>,
) -> Result<EsiImportResult, String> {
    let device_id = default_device_id(device);
    let signals = build_signals(device, warnings);
    if signals.is_empty() {
        warnings.push("未从 ESI 中解析到 PDO Entry，设备 YAML 只包含基本型号信息".to_owned());
    }

    let spec = DeviceSpec {
        id: device_id,
        device_type: device
            .device_type
            .clone()
            .unwrap_or_else(|| "ethercat_slave".to_owned()),
        manufacturer: device.vendor_name.clone(),
        model: model_label(device),
        connection: None,
        network_group: device.network_group.clone(),
        signals,
        alarms: Vec::new(),
    };

    let device_yaml = serde_yaml::to_string(&spec)
        .map_err(|error| format!("DeviceSpec YAML 序列化失败: {error}"))?;
    Ok(EsiImportResult {
        device_yamls: vec![device_yaml],
        warnings: warnings.clone(),
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
                            slave_address: None,
                            signal_prefix: None,
                            entries: Vec::new(),
                        });
                    }
                    "RxPdo" if current_device.is_some() => {
                        current_pdo = Some(EsiPdo {
                            direction: PdoDirection::Rx,
                            index: None,
                            name: None,
                            slave_address: None,
                            signal_prefix: None,
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
