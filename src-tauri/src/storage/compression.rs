use crate::error::{AppError, AppResult};

pub fn compress(raw: &[u8]) -> AppResult<Vec<u8>> {
    zstd::encode_all(raw, 3).map_err(AppError::from)
}

pub fn decompress(compressed: &[u8]) -> AppResult<Vec<u8>> {
    zstd::decode_all(compressed).map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let data = b"hello warp-ade compression test with enough bytes to compress efficiently";
        let compressed = compress(data).unwrap();
        assert_eq!(decompress(&compressed).unwrap(), data);
    }
}
