//! ESI / ENI 共享类型定义。

/// ESI 导入结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EsiImportResult {
    /// 解析产出的设备 YAML 列表（ESI 单设备为 1 个，ENI 多从站为 N 个）。
    pub(crate) device_yamls: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct EsiDevice {
    pub(super) device_type: Option<String>,
    pub(super) vendor_name: Option<String>,
    pub(super) name: Option<String>,
    pub(super) type_name: Option<String>,
    pub(super) product_code: Option<u32>,
    pub(super) revision_no: Option<u32>,
    pub(super) network_group: Option<String>,
    pub(super) tx_pdos: Vec<EsiPdo>,
    pub(super) rx_pdos: Vec<EsiPdo>,
}

#[derive(Debug, Clone)]
pub(super) struct EsiPdo {
    pub(super) direction: PdoDirection,
    pub(super) index: Option<u16>,
    pub(super) name: Option<String>,
    pub(super) slave_address: Option<u16>,
    pub(super) signal_prefix: Option<String>,
    pub(super) entries: Vec<EsiPdoEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PdoDirection {
    /// TxPDO：从站发给主站，建模为设备输入信号。
    Tx,
    /// RxPDO：主站写给从站，建模为设备输出信号。
    Rx,
}

#[derive(Debug, Clone, Default)]
pub(super) struct EsiPdoEntry {
    pub(super) index: Option<u16>,
    pub(super) sub_index: Option<u8>,
    pub(super) bit_len: Option<u16>,
    pub(super) name: Option<String>,
    pub(super) data_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextTarget {
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

#[derive(Debug, Clone, Default)]
pub(super) struct EniSlave {
    pub(super) name: Option<String>,
    pub(super) product_revision: Option<String>,
    pub(super) product_code: Option<u32>,
    pub(super) revision_no: Option<u32>,
    pub(super) phys_addr: Option<u16>,
    pub(super) previous_port: Option<EniPreviousPort>,
    pub(super) tx_pdos: Vec<EsiPdo>,
    pub(super) rx_pdos: Vec<EsiPdo>,
    pub(super) active_rx_pdos: Vec<u16>,
    pub(super) active_tx_pdos: Vec<u16>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct EniPreviousPort {
    pub(super) port: Option<String>,
    pub(super) phys_addr: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EniTextTarget {
    SlaveName,
    ProductRevision,
    ProductCode,
    RevisionNo,
    PhysAddr,
    PreviousPort,
    PreviousPhysAddr,
    Sm2Pdo,
    Sm3Pdo,
    PdoIndex,
    PdoName,
    EntryIndex,
    EntrySubIndex,
    EntryBitLen,
    EntryName,
    EntryDataType,
}
