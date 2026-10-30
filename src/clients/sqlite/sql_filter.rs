/// A SQL `WHERE` condition (without the `WHERE` keyword) and the values to bind to its `?`
/// placeholders, in order.
#[derive(Debug)]
pub struct SqlFilter {
    pub clause: String,
    pub bindings: Vec<String>,
}
