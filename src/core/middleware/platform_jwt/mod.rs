//! Verification of platform-issued access tokens (EdDSA / Ed25519 JWTs minted by
//! the cloud identity service) for a multi-tenant deployment. The identity
//! service publishes its public keys as a JWKS document; `JwksCache` keeps them
//! in memory, refreshes on a TTL, and refreshes early (rate limited) when a
//! token names a `kid` it has not seen, which is how key rotation is picked up
//! without a restart.
//!
//! Local HS256 tokens are untouched: `core::middleware::auth::verify_bearer`
//! dispatches on the token's `alg` header and only EdDSA tokens come through
//! here.

mod middleware_platform_jwt;
#[cfg(test)]
mod tests;

pub use middleware_platform_jwt::JwksCache;
pub use middleware_platform_jwt::PlatformIdentity;
pub use middleware_platform_jwt::cache_for;
pub use middleware_platform_jwt::role_for_scope;
pub use middleware_platform_jwt::verify_platform_token;
