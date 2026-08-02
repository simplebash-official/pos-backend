use mongodb::{
    Client, Database,
    options::{ClientOptions, ResolverConfig},
};

pub async fn connect(uri: &str, db_name: &str) -> mongodb::error::Result<Database> {
    // Some hosts (notably macOS with a link-local IPv6 nameserver like
    // `fe80::...%en0`) ship a system DNS config the driver's resolver can't
    // parse, which breaks `mongodb+srv://` SRV/TXT lookups. Pointing the
    // resolver at a fixed public DNS server sidesteps that instead of
    // relying on whatever the OS reports.
    let mut options = ClientOptions::parse(uri)
        .resolver_config(ResolverConfig::cloudflare())
        .await?;
    options.app_name = Some("jana2u-pos-backend".to_string());

    let client = Client::with_options(options)?;

    // Fail fast on a bad connection string / unreachable server rather than
    // discovering it on the first request.
    client
        .database(db_name)
        .run_command(mongodb::bson::doc! { "ping": 1 })
        .await?;

    Ok(client.database(db_name))
}
