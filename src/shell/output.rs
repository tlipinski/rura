use std::sync::Arc;

#[derive(PartialEq, Debug, Clone)]
pub enum Output {
    Ok(Arc<[u8]>),
    Err(Arc<[u8]>, Option<i32>),
}

#[cfg(test)]
impl Output {
    pub fn ok_str(str: &str) -> Self {
        Output::Ok(str.as_bytes().into())
    }

    pub fn err_str(str: &str) -> Self {
        Output::Err(str.as_bytes().into(), Some(1))
    }
}
