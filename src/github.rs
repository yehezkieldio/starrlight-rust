use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT, IF_NONE_MATCH};
use serde_json::json;

use crate::error::StarredError;
use crate::models::{GraphQLResponse, Repository};
use crate::status::StatusIndicator;
use crate::cache::{CacheManager, CacheConfig};

pub struct GitHubGQL {
    client: reqwest::Client,
    api_url: String,
    cache_manager: Option<CacheManager>,
}

impl GitHubGQL {
    pub fn new(token: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static("starrlight/0.1.0"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        GitHubGQL {
            client,
            api_url: "https://api.github.com/graphql".to_string(),
            cache_manager: None,
        }
    }

    pub fn with_cache(mut self, cache_config: CacheConfig) -> Result<Self, Box<StarredError>> {
        self.cache_manager = Some(CacheManager::new(cache_config)?);
        Ok(self)
    }

    pub async fn get_user_starred_by_username(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_stargazer_count_limit: i32,
        include_private: bool,
        status: &mut StatusIndicator,
    ) -> Result<Vec<Repository>, Box<StarredError>> {
        // Try to get cached data first
        if let Some(cache_manager) = &self.cache_manager {
            status.update_message("Checking cache...");

            if let Some(cached_repos) = cache_manager.get_cached_repositories(
                username,
                limit,
                topic_stargazer_count_limit,
                include_private,
            )? {
                println!("Using cached data - {} repositories found!", cached_repos.len());
                return Ok(cached_repos);
            }

            // Check if we should make a conditional request with ETag
            let etag = cache_manager.get_cached_etag(
                username,
                limit,
                topic_stargazer_count_limit,
                include_private,
            )?;

            if let Some(etag_value) = etag {
                status.update_message("Making conditional request to GitHub API...");

                // Try conditional request first
                if let Ok(conditional_result) = self.make_conditional_request(
                    username,
                    limit,
                    topic_stargazer_count_limit,
                    include_private,
                    &etag_value,
                    status,
                ).await {
                    if let Some(repos) = conditional_result {
                        return Ok(repos);
                    }
                }
            }
        }

        // Make full API request
        status.update_message(&format!("Fetching starred repositories for user: {}", username));
        let repos = self.fetch_repositories_from_api(
            username,
            limit,
            topic_stargazer_count_limit,
            include_private,
            status,
        ).await?;

        // Cache the results if cache manager is available
        if let Some(cache_manager) = &self.cache_manager {
            status.update_message("Caching results...");
            cache_manager.cache_repositories(
                username,
                limit,
                topic_stargazer_count_limit,
                include_private,
                repos.clone(),
                None, // We'll implement ETag extraction later
                None, // Rate limit info
                None, // Rate limit reset
            )?;
        }

        Ok(repos)
    }

    async fn make_conditional_request(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_stargazer_count_limit: i32,
        include_private: bool,
        etag: &str,
        status: &mut StatusIndicator,
    ) -> Result<Option<Vec<Repository>>, Box<StarredError>> {
        // For GraphQL, we'll make a lightweight query to check if data has changed
        let query = r#"
            query ($username: String!) {
                user(login: $username) {
                    starredRepositories(first: 1) {
                        totalCount
                    }
                }
            }
        "#;

        let variables = json!({
            "username": username
        });

        let request_body = json!({
            "query": query,
            "variables": variables
        });

        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_str(etag).unwrap());

        let response = self
            .client
            .post(&self.api_url)
            .headers(headers)
            .json(&request_body)
            .send()
            .await?;

        // If 304 Not Modified, use cached data
        if response.status() == 304 {
            if let Some(cache_manager) = &self.cache_manager {
                if let Some(cached_repos) = cache_manager.get_cached_repositories(
                    username,
                    limit,
                    topic_stargazer_count_limit,
                    include_private,
                )? {
                    status.finish(Some(&format!(
                        "Data unchanged - using cached {} repositories!",
                        cached_repos.len()
                    )));
                    return Ok(Some(cached_repos));
                }
            }
        }

        // Data has changed, return None to trigger full fetch
        Ok(None)
    }

    async fn fetch_repositories_from_api(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_stargazer_count_limit: i32,
        include_private: bool,
        status: &StatusIndicator,
    ) -> Result<Vec<Repository>, Box<StarredError>> {
        let mut items = Vec::new();
        let mut after: Option<String> = None;
        let mut total_fetched = 0;

        loop {
            let query = r#"
                query ($username: String!, $after: String) {
                    user(login: $username) {
                        starredRepositories(first: 100, after: $after, orderBy: {direction: DESC, field: STARRED_AT}) {
                            totalCount
                            nodes {
                                name
                                nameWithOwner
                                description
                                url
                                stargazerCount
                                forkCount
                                isPrivate
                                pushedAt
                                updatedAt
                                languages(first: 1, orderBy: {field: SIZE, direction: DESC}) {
                                    edges {
                                        node {
                                            id
                                            name
                                        }
                                    }
                                }
                                repositoryTopics(first: 100) {
                                    nodes {
                                        topic {
                                            name
                                            stargazerCount
                                        }
                                    }
                                }
                            }
                            pageInfo {
                                endCursor
                                hasNextPage
                            }
                        }
                    }
                }
            "#;

            let variables = json!({
                "username": username,
                "after": after
            });

            let request_body = json!({
                "query": query,
                "variables": variables
            });

            let response = self
                .client
                .post(&self.api_url)
                .json(&request_body)
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(Box::new(StarredError {
                    message: format!("HTTP error: {}", response.status()),
                }));
            }

            let graphql_response: GraphQLResponse = response.json().await?;
            let starred_repos = &graphql_response.data.user.starred_repositories;

            for repo in &starred_repos.nodes {
                if let Some(limit) = limit {
                    if total_fetched >= limit {
                        return Ok(items);
                    }
                }

                // Skip private repos if not requested
                if repo.is_private && !include_private {
                    continue;
                }

                let name = repo.name_with_owner.clone();
                let description = repo.description.clone().unwrap_or_default();
                let language = repo
                    .languages
                    .edges
                    .first()
                    .map(|edge| edge.node.name.clone())
                    .unwrap_or_default();
                let url = repo.url.clone();
                let is_private = repo.is_private;
                let topics: Vec<String> = repo
                    .repository_topics
                    .nodes
                    .iter()
                    .filter(|topic_node| topic_node.topic.stargazer_count > topic_stargazer_count_limit)
                    .map(|topic_node| topic_node.topic.name.clone())
                    .collect();

                items.push(Repository {
                    name,
                    description,
                    language,
                    url,
                    is_private,
                    topics,
                });

                total_fetched += 1;
            }

            status.update_message(&format!(
                "Fetched {} repositories (total: {})",
                starred_repos.nodes.len(),
                total_fetched
            ));

            if starred_repos.page_info.has_next_page {
                after = starred_repos.page_info.end_cursor.clone();
                // Rate limiting - be nice to GitHub's API
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            } else {
                break;
            }
        }

        Ok(items)
    }
}
