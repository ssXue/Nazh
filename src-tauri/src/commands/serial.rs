use std::time::Duration;

use tauri_bindings::{SerialPortInfo, TestSerialResult};

#[tauri::command]
pub(crate) async fn list_serial_ports() -> Result<Vec<SerialPortInfo>, String> {
    let ports = serialport::available_ports().map_err(|e| format!("枚举串口失败: {e}"))?;

    let infos = ports
        .into_iter()
        .map(|port| {
            let path = port.port_name;
            let port_type = classify_serial_port(&path);
            let description = format!("{:?}", port.port_type);
            SerialPortInfo {
                path,
                port_type,
                description,
            }
        })
        .collect();

    Ok(infos)
}

fn classify_serial_port(path: &str) -> String {
    let path_lower = path.to_lowercase();
    if path_lower.contains("bluetooth") || path_lower.contains("bt-") {
        "bluetooth".to_string()
    } else if path_lower.contains("/dev/cu.")
        || path_lower.contains("/dev/tty.")
        || path_lower.contains("/dev/ttyusb")
        || path_lower.contains("/dev/ttyacm")
        || path_lower.contains("/dev/ttyama")
    {
        "usb-serial".to_string()
    } else {
        "builtin".to_string()
    }
}

// TestSerialResult 已迁入 tauri-bindings crate，此处仅使用。

#[tauri::command]
pub(crate) async fn test_serial_connection(
    port_path: String,
    baud_rate: u32,
    data_bits: u8,
    parity: String,
    stop_bits: u8,
    flow_control: String,
) -> Result<TestSerialResult, String> {
    if port_path.trim().is_empty() {
        return Ok(TestSerialResult {
            ok: false,
            message: "端口路径不能为空".to_string(),
        });
    }

    let timeout = Duration::from_secs(3);
    let port_result = serialport::new(port_path.clone(), baud_rate.max(1))
        .timeout(timeout)
        .data_bits(serial_data_bits(data_bits))
        .parity(serial_parity(&parity))
        .stop_bits(serial_stop_bits(stop_bits))
        .flow_control(serial_flow_control(&flow_control))
        .open();

    match port_result {
        Ok(_port) => Ok(TestSerialResult {
            ok: true,
            message: format!("端口 {port_path} 打开成功"),
        }),
        Err(error) => Ok(TestSerialResult {
            ok: false,
            message: format!("端口 {port_path} 打开失败: {error}"),
        }),
    }
}

fn serial_data_bits(value: u8) -> serialport::DataBits {
    match value {
        5 => serialport::DataBits::Five,
        6 => serialport::DataBits::Six,
        7 => serialport::DataBits::Seven,
        _ => serialport::DataBits::Eight,
    }
}

fn serial_parity(value: &str) -> serialport::Parity {
    match value.trim().to_ascii_lowercase().as_str() {
        "odd" | "o" => serialport::Parity::Odd,
        "even" | "e" => serialport::Parity::Even,
        _ => serialport::Parity::None,
    }
}

fn serial_stop_bits(value: u8) -> serialport::StopBits {
    if value == 2 {
        serialport::StopBits::Two
    } else {
        serialport::StopBits::One
    }
}

fn serial_flow_control(value: &str) -> serialport::FlowControl {
    match value.trim().to_ascii_lowercase().as_str() {
        "software" | "xonxoff" => serialport::FlowControl::Software,
        "hardware" | "rtscts" => serialport::FlowControl::Hardware,
        _ => serialport::FlowControl::None,
    }
}
