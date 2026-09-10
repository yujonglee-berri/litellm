mod deployment;
mod strategy;

pub use deployment::{Deployment, LiteLLMParams};
pub use strategy::RoutingStrategy;

#[derive(Clone, Debug, Default)]
pub struct Router {
    model_list: Vec<Deployment>,
    routing_strategy: RoutingStrategy,
}

impl Router {
    pub fn new(model_list: Vec<Deployment>) -> Self {
        Self {
            model_list,
            routing_strategy: RoutingStrategy::SimpleShuffle,
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

    pub fn get_available_deployment(&self, model: &str) -> Option<&Deployment> {
        let candidates: Vec<&Deployment> = self
            .model_list
            .iter()
            .filter(|deployment| deployment.model_name == model)
            .collect();
        self.routing_strategy.select(&candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
