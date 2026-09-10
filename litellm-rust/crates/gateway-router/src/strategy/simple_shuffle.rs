use rand::seq::SliceRandom;

use crate::Deployment;

pub fn select<'a>(candidates: &[&'a Deployment]) -> Option<&'a Deployment> {
    candidates.choose(&mut rand::thread_rng()).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiteLLMParams;

    fn deployment(model: &str) -> Deployment {
        Deployment {
            model_name: "gpt-realtime".to_string(),
            litellm_params: LiteLLMParams {
                model: model.to_string(),
                api_key: None,
                api_base: None,
            },
        }
    }

    #[test]
    fn picks_from_candidates() {
        let a = deployment("key-a");
        let b = deployment("key-b");
        let candidates = vec![&a, &b];
        for _ in 0..20 {
            let chosen = select(&candidates).expect("non-empty");
            assert!(matches!(
                chosen.litellm_params.model.as_str(),
                "key-a" | "key-b"
            ));
        }
    }

    #[test]
    fn empty_candidates_select_none() {
        assert!(select(&[]).is_none());
    }
}
