use std::sync::Arc;

pub enum Output {
    Ok(Arc<[u8]>),
    Err(Arc<[u8]>, Option<i32>),
}
