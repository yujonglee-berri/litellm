use crate::{
    Deployment, Ready, RouteSession, RoutingStateStore, RoutingStrategy, UnconstrainedStateStore,
};

#[derive(Clone, Debug)]
pub struct Router<S = UnconstrainedStateStore> {
    model_list: Vec<Deployment>,
    routing_strategy: RoutingStrategy,
    state_store: S,
}

impl Router<UnconstrainedStateStore> {
    pub fn new(model_list: Vec<Deployment>) -> Self {
        Self {
            model_list,
            routing_strategy: RoutingStrategy::SimpleShuffle,
            state_store: UnconstrainedStateStore,
        }
    }

    pub fn get_available_deployment(&self, model: &str) -> Option<&Deployment> {
        let candidates = self.candidates(model);
        self.routing_strategy.select(&candidates)
    }
}

impl<S> Router<S>
where
    S: RoutingStateStore,
{
    pub fn with_state(model_list: Vec<Deployment>, state_store: S) -> Self {
        Self {
            model_list,
            routing_strategy: RoutingStrategy::SimpleShuffle,
            state_store,
        }
    }

    pub fn deployments(&self) -> &[Deployment] {
        &self.model_list
    }

    pub fn has_deployment(&self, model: &str) -> bool {
        self.model_list
            .iter()
            .any(|deployment| deployment.model_name == model)
    }

    pub fn begin(&self, model: impl Into<String>) -> RouteSession<'_, S, Ready> {
        RouteSession::new(self, model.into())
    }

    pub(crate) fn candidates(&self, model: &str) -> Vec<&Deployment> {
        self.model_list
            .iter()
            .filter(|deployment| deployment.model_name == model)
            .collect()
    }

    pub(crate) fn routing_strategy(&self) -> &RoutingStrategy {
        &self.routing_strategy
    }

    pub(crate) fn state_store(&self) -> &S {
        &self.state_store
    }
}

impl Default for Router<UnconstrainedStateStore> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiteLLMParams;

    fn deployment(name: &str, model: &str) -> Deployment {
        Deployment {
            model_name: name.to_string(),
            litellm_params: LiteLLMParams {
                model: model.to_string(),
                api_key: None,
                api_base: None,
            },
        }
    }

    #[test]
    fn selects_a_matching_deployment() {
        let router = Router::new(vec![
            deployment("gpt-realtime", "gpt-realtime"),
            deployment("other", "other-model"),
        ]);
        let chosen = router
            .get_available_deployment("gpt-realtime")
            .expect("a deployment should match");
        assert_eq!(chosen.model_name, "gpt-realtime");
    }

    #[test]
    fn unknown_model_returns_none() {
        let router = Router::new(vec![deployment("gpt-realtime", "gpt-realtime")]);
        assert!(router.get_available_deployment("missing").is_none());
    }
}
