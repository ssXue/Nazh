//! ENI（EtherCAT Network Information）解析逻辑。

use std::collections::HashMap;

use nazh_dsl_core::device::{ConnectionRef, DeviceSpec};
use quick_xml::Reader;
use quick_xml::events::Event;

use super::types::{EniSlave, EsiDevice, EsiImportResult, EsiPdo, EsiPdoEntry, PdoDirection};
use super::utils::{
    apply_eni_text_target, attr_number, attr_string, build_signals, classify_eni_text_target,
    deterministic_hash, end_name, finish_entry, is_config_slave_path, start_name, to_u8, to_u16,
};

/// ENI 导入入口，由 `esi_import.rs` 在检测到 `EtherCATConfig` 根节点时调用。
pub(super) fn import_eni_to_device_yaml(eni_xml: &str) -> Result<EsiImportResult, String> {
    let slaves = parse_eni_slaves(eni_xml)?;
    if slaves.is_empty() {
        return Err("ENI 文件中没有找到 Config/Slave".to_owned());
    }

    let mut warnings = vec!["检测到 EtherCATConfig/ENI 网络配置，已按激活 SM PDO 导入".to_owned()];

    let group_id = eni_group_id(eni_xml);
    let topology = eni_topology_summary(&slaves);
    if !topology.is_empty() {
        warnings.push(format!("ENI 拓扑: {}", topology.join("；")));
    }

    let mut device_yamls = Vec::new();
    for slave in &slaves {
        let device = eni_slave_to_device(slave, &group_id, &mut warnings);
        let signals = build_signals(&device, &mut warnings);
        if signals.is_empty() {
            warnings.push(format!(
                "{} 没有激活的 PDO Entry，跳过",
                slave.name.as_deref().unwrap_or("未命名从站")
            ));
            continue;
        }

        let spec = DeviceSpec {
            id: eni_slave_device_id(slave),
            device_type: "ethercat_slave".to_owned(),
            manufacturer: slave.product_revision.clone(),
            model: eni_slave_model(slave),
            connection: slave.phys_addr.map(|_addr| ConnectionRef {
                connection_type: "ethercat".to_owned(),
                id: "ethercat_master".to_owned(),
                unit: None,
            }),
            network_group: Some(group_id.clone()),
            signals,
            alarms: Vec::new(),
        };
        let yaml = serde_yaml::to_string(&spec)
            .map_err(|error| format!("从站 DeviceSpec YAML 序列化失败: {error}"))?;
        device_yamls.push(yaml);
    }

    if device_yamls.is_empty() {
        warnings.push("ENI 中没有找到任何激活的 Sm2/Sm3 PDO".to_owned());
    }

    Ok(EsiImportResult {
        device_yamls,
        warnings,
    })
}

#[allow(clippy::too_many_lines)]
fn parse_eni_slaves(eni_xml: &str) -> Result<Vec<EniSlave>, String> {
    let mut reader = Reader::from_str(eni_xml);
    reader.config_mut().trim_text(true);

    let mut path = Vec::<String>::new();
    let mut text = String::new();
    let mut slaves = Vec::new();
    let mut current_slave: Option<EniSlave> = None;
    let mut current_pdo: Option<EsiPdo> = None;
    let mut current_entry: Option<EsiPdoEntry> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                let name = start_name(&element);
                path.push(name.clone());
                text.clear();
                match name.as_str() {
                    "Slave" if is_config_slave_path(&path) => {
                        current_slave = Some(EniSlave::default());
                    }
                    "TxPdo" if current_slave.is_some() => {
                        current_pdo = Some(EsiPdo {
                            direction: PdoDirection::Tx,
                            index: None,
                            name: None,
                            slave_address: None,
                            signal_prefix: None,
                            entries: Vec::new(),
                        });
                    }
                    "RxPdo" if current_slave.is_some() => {
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
                    .map_err(|error| format!("ENI 文本解码失败: {error}"))?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map_err(|error| format!("ENI 文本转义解析失败: {error}"))?;
                text.push_str(&unescaped);
            }
            Ok(Event::CData(event)) => {
                let decoded = event
                    .decode()
                    .map_err(|error| format!("ENI CDATA 解码失败: {error}"))?;
                text.push_str(&decoded);
            }
            Ok(Event::End(element)) => {
                let name = end_name(&element);
                if let Some(target) = classify_eni_text_target(&path, &name) {
                    apply_eni_text_target(
                        target,
                        text.trim(),
                        &mut current_slave,
                        &mut current_pdo,
                        &mut current_entry,
                    );
                }
                match name.as_str() {
                    "Entry" => finish_entry(&mut current_pdo, &mut current_entry),
                    "TxPdo" | "RxPdo" => {
                        if let (Some(slave), Some(pdo)) =
                            (current_slave.as_mut(), current_pdo.take())
                        {
                            match pdo.direction {
                                PdoDirection::Tx => slave.tx_pdos.push(pdo),
                                PdoDirection::Rx => slave.rx_pdos.push(pdo),
                            }
                        }
                    }
                    "Slave" if current_slave.is_some() && is_config_slave_path(&path) => {
                        if let Some(slave) = current_slave.take() {
                            slaves.push(slave);
                        }
                    }
                    _ => {}
                }
                let _ = path.pop();
                text.clear();
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("ENI XML 解析失败: {error}")),
            _ => {}
        }
    }

    Ok(slaves)
}

/// 将单个 ENI 从站转为 EsiDevice（包含该从站的激活 PDO）。
fn eni_slave_to_device(slave: &EniSlave, group_id: &str, warnings: &mut Vec<String>) -> EsiDevice {
    let mut device = EsiDevice {
        device_type: Some("ethercat_slave".to_owned()),
        vendor_name: None,
        name: slave.name.clone(),
        type_name: slave.name.clone(),
        product_code: slave.product_code,
        revision_no: slave.revision_no,
        network_group: Some(group_id.to_owned()),
        tx_pdos: Vec::new(),
        rx_pdos: Vec::new(),
    };

    let Some(slave_address) = slave.phys_addr else {
        warnings.push(format!(
            "{} 缺少 PhysAddr，无法关联激活 PDO",
            slave.name.as_deref().unwrap_or("未命名从站")
        ));
        return device;
    };

    append_active_eni_pdos(
        &mut device,
        slave,
        slave_address,
        PdoDirection::Tx,
        warnings,
    );
    append_active_eni_pdos(
        &mut device,
        slave,
        slave_address,
        PdoDirection::Rx,
        warnings,
    );

    device
}

fn append_active_eni_pdos(
    device: &mut EsiDevice,
    slave: &EniSlave,
    slave_address: u16,
    direction: PdoDirection,
    warnings: &mut Vec<String>,
) {
    let (active, pdos, target) = match direction {
        PdoDirection::Tx => (&slave.active_tx_pdos, &slave.tx_pdos, &mut device.tx_pdos),
        PdoDirection::Rx => (&slave.active_rx_pdos, &slave.rx_pdos, &mut device.rx_pdos),
    };
    let pdo_by_index: HashMap<u16, &EsiPdo> = pdos
        .iter()
        .filter_map(|pdo| pdo.index.map(|index| (index, pdo)))
        .collect();

    for pdo_index in active {
        let Some(source) = pdo_by_index.get(pdo_index) else {
            warnings.push(format!(
                "{} 激活了 PDO 0x{pdo_index:04X}，但 ProcessData 中缺少定义",
                slave.name.as_deref().unwrap_or("未命名从站")
            ));
            continue;
        };
        let mut pdo = (*source).clone();
        pdo.slave_address = Some(slave_address);
        pdo.signal_prefix = None;
        target.push(pdo);
    }
}

/// 从 ENI 内容生成确定性组 ID。
fn eni_group_id(eni_xml: &str) -> String {
    let hash = deterministic_hash(eni_xml.as_bytes());
    format!("eni_{:06x}", hash % 0x00FF_FFFF)
}

/// 从 ENI 从站信息派生设备 ID。
fn eni_slave_device_id(slave: &EniSlave) -> String {
    if let Some(product_code) = slave.product_code {
        if let Some(addr) = slave.phys_addr {
            return format!("{:06x}_{}", product_code % 1_000_000, addr);
        }
        return format!("{:06}", product_code % 1_000_000);
    }
    let label = slave.name.as_deref().unwrap_or("ethercat_slave");
    let hash = deterministic_hash(label.as_bytes());
    format!("{:06}", hash % 1_000_000)
}

/// 从 ENI 从站信息派生 model 标签。
fn eni_slave_model(slave: &EniSlave) -> Option<String> {
    let base = slave.name.as_deref()?;
    let mut parts = Vec::new();
    if let Some(product_code) = slave.product_code {
        parts.push(format!("ProductCode 0x{product_code:08X}"));
    }
    if let Some(revision_no) = slave.revision_no {
        parts.push(format!("Revision 0x{revision_no:08X}"));
    }
    if let Some(addr) = slave.phys_addr {
        parts.push(format!("Addr {addr}"));
    }
    if parts.is_empty() {
        Some(base.to_owned())
    } else {
        Some(format!("{base} ({})", parts.join(", ")))
    }
}

fn eni_topology_summary(slaves: &[EniSlave]) -> Vec<String> {
    let names_by_addr: HashMap<u16, &str> = slaves
        .iter()
        .filter_map(|slave| {
            Some((
                slave.phys_addr?,
                slave.name.as_deref().unwrap_or("未命名从站"),
            ))
        })
        .collect();
    slaves
        .iter()
        .filter_map(|slave| {
            let addr = slave.phys_addr?;
            let name = slave.name.as_deref().unwrap_or("未命名从站");
            let Some(previous) = &slave.previous_port else {
                return Some(format!("{addr} {name} 直接挂主站"));
            };
            let parent = previous.phys_addr?;
            let parent_name = names_by_addr.get(&parent).copied().unwrap_or("未知从站");
            let port = previous.port.as_deref().unwrap_or("未知端口");
            Some(format!(
                "{addr} {name} <- {parent} {parent_name} / Port {port}"
            ))
        })
        .collect()
}
