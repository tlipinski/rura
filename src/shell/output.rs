#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub lines: Vec<String>,
    pub bytes: Vec<u8>,
    pub status_code: Option<i32>,
    pub ok: bool,
}

impl Output {
    pub fn ok(bytes: Vec<u8>) -> Self {
        Self {
            lines: Self::lines(&String::from_utf8_lossy(&bytes)),
            bytes,
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err(bytes: Vec<u8>, status_code: Option<i32>) -> Self {
        Self {
            lines: Self::lines(&String::from_utf8_lossy(&bytes)),
            bytes,
            status_code,
            ok: false,
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    fn lines(input: &str) -> Vec<String> {
        input.lines().map(|a| a.into()).collect()
    }
}

#[cfg(test)]
impl Output {
    pub fn ok_str(str: &str) -> Self {
        Self {
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err_str(str: &str, status_code: Option<i32>) -> Self {
        Self {
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
            status_code,
            ok: false,
        }
    }
}
