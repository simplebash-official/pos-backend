// Registers a tenant in the platform directory (multi-tenant deployments):
//   cargo run --bin create_tenant -- <shop-code> "<Business name>" [tenant-id]
// Pass the tenant id the identity service issued (tnt_...) so staff logins by
// shop code reach the same data the devices sync; without it a new id is minted.
// Requires TENANT_MODE=multi on MongoDB (see `.env.web.example`).

use simplebash_pos_backend::{
    clients,
    core::config::{Config, TenantMode},
    modules::tenants::service,
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let mut args = std::env::args().skip(1);
    let (Some(shop_code), Some(name)) = (args.next(), args.next()) else {
        eprintln!("usage: create_tenant <shop-code> \"<Business name>\" [tenant-id]");
        std::process::exit(2);
    };
    let tenant_id = args.next();

    let config = Config::from_env().expect("invalid configuration");
    if config.tenant_mode != TenantMode::Multi {
        eprintln!("create_tenant needs TENANT_MODE=multi (and DATABASE_TYPE=mongodb)");
        std::process::exit(2);
    }
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to the database");
    if let Some(mongo) = db.as_mongo() {
        clients::indexes::ensure_indexes(mongo.unscoped(), true).await;
    }

    let created = match tenant_id {
        Some(id) => service::create_tenant_with_key(&db, id, &shop_code, &name).await,
        None => service::create_tenant(&db, &shop_code, &name).await,
    };
    match created {
        Ok(t) => println!("created tenant {} (shop code '{}')", t.key, t.shop_code),
        Err(err) => {
            eprintln!("could not create tenant: {err}");
            std::process::exit(1);
        }
    }
}
