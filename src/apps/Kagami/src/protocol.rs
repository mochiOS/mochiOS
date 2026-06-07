/// Wayland メッセージヘッダー
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    pub object_id: u32,
    pub opcode: u16,
    pub size: u16,
}

impl MessageHeader {
    pub fn new(object_id: u32, opcode: u16, data_size: u16) -> Self {
        Self {
            object_id,
            opcode,
            size: 8 + data_size,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }

        let object_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let size = u16::from_le_bytes([bytes[4], bytes[5]]);
        let opcode = u16::from_le_bytes([bytes[6], bytes[7]]);

        Some(MessageHeader {
            object_id,
            opcode,
            size,
        })
    }

    pub fn to_bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&self.object_id.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.size.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.opcode.to_le_bytes());
        bytes
    }
}

/// Wayland メッセージ
#[derive(Debug, Clone)]
pub struct Message {
    pub header: MessageHeader,
    pub data: Vec<u8>,
}

impl Message {
    pub fn new(object_id: u32, opcode: u16, data: Vec<u8>) -> Self {
        let header = MessageHeader::new(object_id, opcode, data.len() as u16);
        Message { header, data }
    }

    /// メッセージを バイト列にシリアライズ
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.header.size as usize);
        result.extend_from_slice(&self.header.to_bytes());
        result.extend_from_slice(&self.data);
        result
    }

    /// バイト列からパース
    pub fn from_bytes(bytes: &[u8]) -> Option<(Self, usize)> {
        let header = MessageHeader::from_bytes(bytes)?;
        let size = header.size as usize;

        if bytes.len() < size {
            return None;
        }

        let data = bytes[8..size].to_vec();
        let message = Message { header, data };

        Some((message, size))
    }
}

// ========== メッセージビルダー ==========

pub struct MessageBuilder {
    object_id: u32,
    opcode: u16,
    data: Vec<u8>,
}

impl MessageBuilder {
    pub fn new(object_id: u32, opcode: u16) -> Self {
        MessageBuilder {
            object_id,
            opcode,
            data: Vec::new(),
        }
    }

    pub fn push_u32(mut self, value: u32) -> Self {
        self.data.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn push_i32(mut self, value: i32) -> Self {
        self.data.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn push_string(mut self, value: &str) -> Self {
        let len = (value.len() + 1) as u32;
        self.data.extend_from_slice(&len.to_le_bytes());
        self.data.extend_from_slice(value.as_bytes());
        self.data.push(0);
        // パディング
        let padding = (4 - ((len as usize) % 4)) % 4;
        self.data.extend(std::iter::repeat(0).take(padding));
        self
    }

    pub fn push_bytes(mut self, bytes: &[u8]) -> Self {
        self.data.extend_from_slice(bytes);
        self
    }

    pub fn build(self) -> Message {
        Message::new(self.object_id, self.opcode, self.data)
    }
}

// ========== メッセージパーサー ==========

pub struct MessageParser<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> MessageParser<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        MessageParser { data, offset: 0 }
    }

    pub fn read_u32(&mut self) -> Option<u32> {
        if self.offset + 4 > self.data.len() {
            return None;
        }
        let value = u32::from_le_bytes([
            self.data[self.offset],
            self.data[self.offset + 1],
            self.data[self.offset + 2],
            self.data[self.offset + 3],
        ]);
        self.offset += 4;
        Some(value)
    }

    pub fn read_i32(&mut self) -> Option<i32> {
        self.read_u32().map(|v| v as i32)
    }

    pub fn read_string(&mut self) -> Option<String> {
        let len = self.read_u32()? as usize;
        if self.offset + len > self.data.len() {
            return None;
        }
        let s = std::str::from_utf8(&self.data[self.offset..self.offset + len - 1])
            .ok()?
            .to_string();
        self.offset += len;
        // アラインメント
        let padding = (4 - (len % 4)) % 4;
        self.offset += padding;
        Some(s)
    }

    pub fn read_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        if self.offset + len > self.data.len() {
            return None;
        }
        let bytes = &self.data[self.offset..self.offset + len];
        self.offset += len;
        Some(bytes)
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_header_serialization() {
        let header = MessageHeader::new(1, 0, 4);
        let bytes = header.to_bytes();
        assert_eq!(bytes[0], 1);
        assert_eq!(bytes[4], 12);
    }

    #[test]
    fn test_message_builder() {
        let msg = MessageBuilder::new(1, 0)
            .push_u32(42)
            .push_i32(-10)
            .build();

        assert_eq!(msg.header.object_id, 1);
        assert_eq!(msg.header.opcode, 0);
    }

    #[test]
    fn test_message_parser() {
        let data = vec![42u8, 0, 0, 0];
        let mut parser = MessageParser::new(&data);
        let value = parser.read_u32();
        assert_eq!(value, Some(42));
    }

    #[test]
    fn test_message_roundtrip() {
        let original = MessageBuilder::new(5, 3)
            .push_u32(100)
            .push_i32(-50)
            .build();

        let bytes = original.to_bytes();
        let (parsed, size) = Message::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.header.object_id, original.header.object_id);
        assert_eq!(parsed.header.opcode, original.header.opcode);
        assert_eq!(size, bytes.len());
    }
}