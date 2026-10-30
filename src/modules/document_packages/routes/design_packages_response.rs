use super::design_package::DesignPackage;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DesignPackagesResponse {
    pub packages: Vec<DesignPackage>,
}
