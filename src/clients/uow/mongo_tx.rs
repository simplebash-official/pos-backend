use super::super::tenant_db::TenantDatabase;

use mongodb::ClientSession;

#[derive(Debug)]
pub(super) struct MongoTx {
    pub(super) db: TenantDatabase,
    pub(super) session: ClientSession,
}
