//! 轻量十六进制编解码工具。
//!
//! 不引入外部 `hex` crate，自行实现以避免依赖膨胀。

const HEX_CHARS: [u8; 16] = [
    b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'A', b'B', b'C', b'D', b'E', b'F',
];

/// 将字节数组编码为大写十六进制字符串。
pub fn encode(data: &[u8]) -> String {
    let mut result = String::with_capacity(data.len() * 2);
    for byte in data {
        result.push(char::from(HEX_CHARS[(byte >> 4) as usize]));
        result.push(char::from(HEX_CHARS[(byte & 0x0F) as usize]));
    }
    result
}

/// 将十六进制字符串解码为字节数组。
pub fn decode(hex: &str) -> Result<Vec<u8>, String> {
    let bytes = hex.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err("十六进制字符串长度必须是偶数".to_owned());
    }

    let mut result = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks(2) {
        let hi = decode_hex_digit(chunk[0])?;
        let lo = decode_hex_digit(chunk[1])?;
        result.push((hi << 4) | lo);
    }
    Ok(result)
}

fn decode_hex_digit(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(format!("非法十六进制字符: {c}")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn 编码字节数组() {
        assert_eq!(encode(&[0x01, 0x02, 0x03]), "010203");
        assert_eq!(encode(&[0xAB, 0xCD, 0xEF]), "ABCDEF");
    }

    #[test]
    fn 解码十六进制字符串() {
        assert_eq!(decode("010203").unwrap(), vec![0x01, 0x02, 0x03]);
        assert_eq!(decode("abcdef").unwrap(), vec![0xAB, 0xCD, 0xEF]);
        assert_eq!(decode("ABCDEF").unwrap(), vec![0xAB, 0xCD, 0xEF]);
    }

    #[test]
    fn 解码奇数长度失败() {
        assert!(decode("123").is_err());
    }

    #[test]
    fn 解码非法字符失败() {
        assert!(decode("01G2").is_err());
    }
}
