use crate::domain::employees::EmployeeLoginSummary;

use crate::modules::users::model::UserDocument;

pub(super) fn to_login_summary(document: UserDocument) -> EmployeeLoginSummary {
    EmployeeLoginSummary {
        user_id: document
            .id
            .expect("persisted user document must have an id")
            .to_hex(),
        username: document.username,
        role: document.role,
        is_active: document.is_active,
    }
}
