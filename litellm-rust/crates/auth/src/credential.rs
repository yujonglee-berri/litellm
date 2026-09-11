use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use veil::Redact;

use crate::AuthError;

use super::{ResolvedCredential, SecretValue, TokenProviderHandle};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialFileRef {
    Path(PathBuf),
    EnvironmentVariable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialRef {
    Explicit(SecretValue),
    Env(String),
    File(CredentialFileRef),
    Request(String),
    Host(String),
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialSource {
    Deployment,
    Environment,
    File,
    Dynamic,
    Host,
    Caller,
    Sdk,
}

#[derive(Clone, Debug)]
pub enum CredentialLocation {
    Static(SecretValue),
    Environment(String),
    File(PathBuf),
    FileFromEnvironment(String),
    Dynamic(String),
    Host(String),
    Caller(TokenProviderHandle),
    Sdk(String),
    None,
}

impl CredentialLocation {
    pub fn compile<E, F>(&self, environment: &E, file: &F) -> Result<CredentialPlan, AuthError>
    where
        E: Fn(&str) -> Result<Option<String>, AuthError> + ?Sized,
        F: Fn(&Path) -> Result<Option<String>, AuthError> + ?Sized,
    {
        Ok(match self {
            Self::Static(secret) => CredentialPlan::Resolved {
                credential: ResolvedCredential::Static(secret.clone()),
                source: CredentialSource::Deployment,
            },
            Self::Environment(name) => {
                optional_secret(environment(name)?, CredentialSource::Environment)
            }
            Self::File(path) => optional_secret(file(path)?, CredentialSource::File),
            Self::FileFromEnvironment(name) => environment(name)?
                .filter(|path| !path.trim().is_empty())
                .map(|path| file(Path::new(&path)))
                .transpose()?
                .flatten()
                .map_or(CredentialPlan::None, |value| {
                    optional_secret(Some(value), CredentialSource::File)
                }),
            Self::Dynamic(name) => CredentialPlan::Reference(CredentialRef::Request(name.clone())),
            Self::Host(name) => CredentialPlan::Reference(CredentialRef::Host(name.clone())),
            Self::Caller(caller) => CredentialPlan::Caller(caller.clone()),
            Self::Sdk(name) => CredentialPlan::Sdk(name.clone()),
            Self::None => CredentialPlan::None,
        })
    }
}

fn optional_secret(value: Option<String>, source: CredentialSource) -> CredentialPlan {
    value
        .filter(|value| !value.trim().is_empty())
        .map_or(CredentialPlan::None, |value| CredentialPlan::Resolved {
            credential: ResolvedCredential::Static(SecretValue::new(value)),
            source,
        })
}

#[derive(Clone, Default)]
pub struct DynamicCredentials(Arc<BTreeMap<String, SecretValue>>);

impl DynamicCredentials {
    pub fn new(credentials: impl IntoIterator<Item = (String, SecretValue)>) -> Self {
        Self(Arc::new(credentials.into_iter().collect()))
    }

    pub fn get(&self, name: &str) -> Option<&SecretValue> {
        self.0.get(name)
    }
}

impl std::fmt::Debug for DynamicCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamicCredentials")
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialLookup {
    Found(SecretValue),
    Missing,
    Declined,
}

pub type CredentialLookupFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CredentialLookup, AuthError>> + Send + 'a>>;

pub trait CredentialResolver: std::fmt::Debug + Send + Sync {
    fn resolve<'a>(&'a self, reference: &'a CredentialRef) -> CredentialLookupFuture<'a>;

    fn resolve_sdk<'a>(&'a self, _name: &'a str) -> CredentialLookupFuture<'a> {
        Box::pin(async { Ok(CredentialLookup::Declined) })
    }
}

impl CredentialResolver for DynamicCredentials {
    fn resolve<'a>(&'a self, reference: &'a CredentialRef) -> CredentialLookupFuture<'a> {
        Box::pin(async move {
            let name = match reference {
                CredentialRef::Request(name) => name,
                _ => return Ok(CredentialLookup::Declined),
            };
            Ok(self
                .get(name)
                .cloned()
                .map_or(CredentialLookup::Missing, CredentialLookup::Found))
        })
    }
}

#[derive(Clone, Redact)]
pub struct CredentialResolverHandle(#[redact(with = "[REDACTED]")] Arc<dyn CredentialResolver>);

impl CredentialResolverHandle {
    pub fn new(resolver: Arc<dyn CredentialResolver>) -> Self {
        Self(resolver)
    }

    pub async fn resolve(&self, reference: &CredentialRef) -> Result<CredentialLookup, AuthError> {
        self.0.resolve(reference).await
    }
}

impl CredentialResolver for CredentialResolverHandle {
    fn resolve<'a>(&'a self, reference: &'a CredentialRef) -> CredentialLookupFuture<'a> {
        self.0.resolve(reference)
    }

    fn resolve_sdk<'a>(&'a self, name: &'a str) -> CredentialLookupFuture<'a> {
        self.0.resolve_sdk(name)
    }
}

#[derive(Clone, Debug)]
pub enum CredentialPlan {
    Resolved {
        credential: ResolvedCredential,
        source: CredentialSource,
    },
    Reference(CredentialRef),
    Caller(TokenProviderHandle),
    Fallback(Vec<CredentialPlan>),
    Sdk(String),
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialPlanResolution {
    Resolved {
        credential: ResolvedCredential,
        source: CredentialSource,
    },
    Unavailable,
}

impl CredentialPlanResolution {
    pub fn source(&self) -> Option<CredentialSource> {
        match self {
            Self::Resolved { source, .. } => Some(*source),
            Self::Unavailable => None,
        }
    }
}

impl CredentialPlan {
    pub fn fallback(plans: impl IntoIterator<Item = CredentialPlan>) -> Self {
        Self::Fallback(plans.into_iter().collect())
    }

    pub fn resolve<'a, R>(
        &'a self,
        resolver: &'a R,
    ) -> Pin<Box<dyn Future<Output = Result<CredentialPlanResolution, AuthError>> + Send + 'a>>
    where
        R: CredentialResolver + ?Sized,
    {
        Box::pin(async move {
            match self {
                Self::Resolved { credential, source } => Ok(CredentialPlanResolution::Resolved {
                    credential: credential.clone(),
                    source: *source,
                }),
                Self::Reference(CredentialRef::Explicit(secret)) => {
                    Ok(CredentialPlanResolution::Resolved {
                        credential: ResolvedCredential::Static(secret.clone()),
                        source: CredentialSource::Deployment,
                    })
                }
                Self::Reference(CredentialRef::None) | Self::None => {
                    Ok(CredentialPlanResolution::Unavailable)
                }
                Self::Reference(reference) => resolve_reference(resolver, reference).await,
                Self::Caller(caller) => {
                    let credential = caller.acquire().await?;
                    if credential.secret().expose().is_empty() {
                        return Err(AuthError::EmptyCallerCredential);
                    }
                    Ok(CredentialPlanResolution::Resolved {
                        credential,
                        source: CredentialSource::Caller,
                    })
                }
                Self::Fallback(plans) => {
                    for plan in plans {
                        match plan.resolve(resolver).await? {
                            CredentialPlanResolution::Resolved { credential, source } => {
                                return Ok(CredentialPlanResolution::Resolved {
                                    credential,
                                    source,
                                });
                            }
                            CredentialPlanResolution::Unavailable => {}
                        }
                    }
                    Ok(CredentialPlanResolution::Unavailable)
                }
                Self::Sdk(name) => resolve_sdk(resolver, name).await,
            }
        })
    }
}

async fn resolve_reference<R>(
    resolver: &R,
    reference: &CredentialRef,
) -> Result<CredentialPlanResolution, AuthError>
where
    R: CredentialResolver + ?Sized,
{
    let source = match reference {
        CredentialRef::Explicit(_) => CredentialSource::Deployment,
        CredentialRef::Env(_) => CredentialSource::Environment,
        CredentialRef::File(_) => CredentialSource::File,
        CredentialRef::Request(_) => CredentialSource::Dynamic,
        CredentialRef::Host(_) => CredentialSource::Host,
        CredentialRef::None => return Ok(CredentialPlanResolution::Unavailable),
    };
    resolve_lookup(resolver.resolve(reference).await?, source)
}

async fn resolve_sdk<R>(resolver: &R, name: &str) -> Result<CredentialPlanResolution, AuthError>
where
    R: CredentialResolver + ?Sized,
{
    resolve_lookup(resolver.resolve_sdk(name).await?, CredentialSource::Sdk)
}

fn resolve_lookup(
    lookup: CredentialLookup,
    source: CredentialSource,
) -> Result<CredentialPlanResolution, AuthError> {
    match lookup {
        CredentialLookup::Found(secret) if !secret.expose().trim().is_empty() => {
            Ok(CredentialPlanResolution::Resolved {
                credential: ResolvedCredential::Static(secret),
                source,
            })
        }
        CredentialLookup::Found(_) | CredentialLookup::Missing => {
            Ok(CredentialPlanResolution::Unavailable)
        }
        CredentialLookup::Declined => Err(AuthError::Configuration(
            crate::AuthConfigurationError::UnsupportedCredentialReference,
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Debug)]
    struct HostResolver;

    impl CredentialResolver for HostResolver {
        fn resolve<'a>(&'a self, reference: &'a CredentialRef) -> CredentialLookupFuture<'a> {
            Box::pin(async move {
                Ok(match reference {
                    CredentialRef::Host(name) if name == "rotating-token" => {
                        CredentialLookup::Found(SecretValue::new("resolved"))
                    }
                    CredentialRef::Request(_) => CredentialLookup::Missing,
                    _ => CredentialLookup::Declined,
                })
            })
        }
    }

    #[tokio::test]
    async fn dynamic_credentials_are_isolated_and_missing_is_unavailable() {
        let first = DynamicCredentials::new([("api-key".into(), SecretValue::new("first"))]);
        let second = DynamicCredentials::new([("api-key".into(), SecretValue::new("second"))]);
        let plan = CredentialPlan::Reference(CredentialRef::Request("api-key".into()));

        let first_value = plan.resolve(&first).await.unwrap();
        let second_value = plan.resolve(&second).await.unwrap();
        let missing = CredentialPlan::Reference(CredentialRef::Request("missing".into()))
            .resolve(&first)
            .await
            .unwrap();

        assert_eq!(resolved_secret(first_value).as_deref(), Some("first"));
        assert_eq!(resolved_secret(second_value).as_deref(), Some("second"));
        assert_eq!(
            plan.resolve(&first).await.unwrap().source(),
            Some(CredentialSource::Dynamic)
        );
        assert_eq!(missing, CredentialPlanResolution::Unavailable);
    }

    #[test]
    fn dynamic_credentials_debug_is_redacted() {
        let credentials =
            DynamicCredentials::new([("api-key".into(), SecretValue::new("plain-secret"))]);
        let debug = format!("{credentials:?}");

        assert!(!debug.contains("plain-secret"));
        assert!(debug.contains("REDACTED"));
    }

    #[tokio::test]
    async fn environment_and_files_compile_once_through_injected_lookups() {
        let environment_calls = AtomicUsize::new(0);
        let file_calls = AtomicUsize::new(0);
        let environment = |name: &str| {
            environment_calls.fetch_add(1, Ordering::SeqCst);
            Ok(match name {
                "DIRECT" => Some("environment-secret".into()),
                "FILE_PATH" => Some("/injected/secret".into()),
                _ => None,
            })
        };
        let file = |path: &Path| {
            file_calls.fetch_add(1, Ordering::SeqCst);
            Ok((path == Path::new("/injected/secret")).then(|| "file-secret".into()))
        };

        let environment_plan = CredentialLocation::Environment("DIRECT".into())
            .compile(&environment, &file)
            .unwrap();
        let direct_file_plan = CredentialLocation::File("/injected/secret".into())
            .compile(&environment, &file)
            .unwrap();
        let file_plan = CredentialLocation::FileFromEnvironment("FILE_PATH".into())
            .compile(&environment, &file)
            .unwrap();

        assert_eq!(environment_calls.load(Ordering::SeqCst), 2);
        assert_eq!(file_calls.load(Ordering::SeqCst), 2);
        environment_plan.resolve(&HostResolver).await.unwrap();
        environment_plan.resolve(&HostResolver).await.unwrap();
        direct_file_plan.resolve(&HostResolver).await.unwrap();
        file_plan.resolve(&HostResolver).await.unwrap();
        assert_eq!(environment_calls.load(Ordering::SeqCst), 2);
        assert_eq!(file_calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            environment_plan
                .resolve(&HostResolver)
                .await
                .unwrap()
                .source(),
            Some(CredentialSource::Environment)
        );
        assert!(matches!(
            environment_plan,
            CredentialPlan::Resolved {
                source: CredentialSource::Environment,
                ..
            }
        ));
        assert!(matches!(
            direct_file_plan,
            CredentialPlan::Resolved {
                source: CredentialSource::File,
                ..
            }
        ));
        assert!(matches!(
            file_plan,
            CredentialPlan::Resolved {
                source: CredentialSource::File,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn fallback_continues_only_when_unavailable() {
        let plan = CredentialPlan::fallback([
            CredentialPlan::Reference(CredentialRef::Request("missing".into())),
            CredentialPlan::Reference(CredentialRef::Host("rotating-token".into())),
        ]);

        assert_eq!(
            resolved_secret(plan.resolve(&HostResolver).await.unwrap()).as_deref(),
            Some("resolved")
        );
    }

    #[derive(Debug)]
    struct FailingResolver;

    impl CredentialResolver for FailingResolver {
        fn resolve<'a>(&'a self, _reference: &'a CredentialRef) -> CredentialLookupFuture<'a> {
            Box::pin(async { Err(AuthError::UnresolvedOidcReference) })
        }
    }

    #[tokio::test]
    async fn fallback_does_not_swallow_resolution_errors() {
        let plan = CredentialPlan::fallback([
            CredentialPlan::Reference(CredentialRef::Host("failure".into())),
            CredentialPlan::Resolved {
                credential: ResolvedCredential::Static(SecretValue::new("fallback")),
                source: CredentialSource::Deployment,
            },
        ]);

        assert_eq!(
            plan.resolve(&FailingResolver).await.unwrap_err(),
            AuthError::UnresolvedOidcReference
        );
    }

    #[test]
    fn environment_and_file_load_errors_are_terminal_during_compilation() {
        let load_error = || AuthError::Configuration(crate::AuthConfigurationError::CredentialLoad);

        let environment_error = CredentialLocation::Environment("API_KEY".into())
            .compile(&|_| Err(load_error()), &|_| Ok(None))
            .unwrap_err();
        let file_error = CredentialLocation::File("/secret".into())
            .compile(&|_| Ok(None), &|_| Err(load_error()))
            .unwrap_err();

        assert_eq!(environment_error, load_error());
        assert_eq!(file_error, load_error());
    }

    #[tokio::test]
    async fn unsupported_sdk_remains_deferred() {
        let plan = CredentialLocation::Sdk("provider-sdk".into())
            .compile(&|_| Ok(None), &|_| Ok(None))
            .unwrap();

        assert_eq!(
            plan.resolve(&HostResolver).await.unwrap_err(),
            AuthError::Configuration(crate::AuthConfigurationError::UnsupportedCredentialReference)
        );
    }

    fn resolved_secret(resolution: CredentialPlanResolution) -> Option<String> {
        match resolution {
            CredentialPlanResolution::Resolved { credential, .. } => {
                Some(credential.secret().expose().to_string())
            }
            CredentialPlanResolution::Unavailable => None,
        }
    }
}
