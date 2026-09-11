use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct K8sConfig {
    pub interval_seconds: u64,
    pub old_match_delete_after_minutes: i64,
    pub job_prefix: String,
    pub website_url: String,
    pub namespace: String,
    pub arenaclients_json_path: String,
    pub version: String,
    pub max_arenaclients: usize,
    #[serde(default = "default_caching_server_url")]
    pub caching_server_url: String,
}

fn default_caching_server_url() -> String {
    "http://aiarena-caching-nodeport-service".to_string()
}

#[cfg(test)]
mod tests {
    use super::default_caching_server_url;

    #[test]
    fn default_cache_url_matches_match_controller_service() {
        assert_eq!(default_caching_server_url(), "http://aiarena-caching-nodeport-service");
    }
}
