/// Who a batch of changes came from; stamps `updated_by_device` / row meta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOrigin {
    Device(String),
    Cloud,
}
