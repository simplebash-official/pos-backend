/// Geometry shared by every label element kind, in millimetres.
#[derive(Debug, Clone, Copy)]
pub struct LabelFrame<'element> {
    pub id: &'element str,
    pub x_mm: f64,
    pub y_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
}
