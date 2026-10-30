/// `snake_case` column name -> `camelCase` DTO field name.
#[must_use]
pub fn snake_to_camel(column: &str) -> String {
    let mut out = String::with_capacity(column.len());
    let mut upper = false;
    for c in column.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}
