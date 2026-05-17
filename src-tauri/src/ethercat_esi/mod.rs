//! `EtherCAT` ESI / ENI 文件导入器。
//!
//! ESI（EtherCAT Slave Information）是设备厂商发布的 XML 描述文件。
//! ENI（EtherCAT Network Information）是主站导出的网络配置文件。
//! 本模块只做确定性解析：提取厂商、型号、ProductCode、RevisionNo、拓扑与 PDO 条目，
//! 再转换为 Device DSL YAML，供用户审查后保存为设备资产。

mod eni_import;
mod esi_import;
pub(crate) mod types;
mod utils;

pub(crate) use esi_import::import_esi_to_device_yaml;
