// Verification of platform-issued access tokens (EdDSA / Ed25519 JWTs minted by
// the cloud identity service) for a multi-tenant deployment. The identity
// service publishes its public keys as a JWKS document; `JwksCache` keeps them
// in memory, refreshes on a TTL, and refreshes early (rate limited) when a
// token names a `kid` it has not seen, which is how key rotation is picked up
// without a restart.
//
// Local HS256 tokens are untouched: `core::middleware::auth::verify_bearer`
// dispatches on the token's `alg` header and only EdDSA tokens come through
// here.

mod cache_for;
mod jwks_cache;
mod platform_identity;
mod role_for_scope;
#[cfg(test)]
mod tests;
mod verify_platform_token;

pub use cache_for::cache_for;
pub use jwks_cache::JwksCache;
pub use platform_identity::PlatformIdentity;
pub use role_for_scope::role_for_scope;
pub use verify_platform_token::verify_platform_token;
