use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
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
        let cache_key = if let Some(cache_manager) = &self.cache_manager {
            cache_manager.generate_cache_key(username, limit, topic_stargazer_count_limit, include_private)
        } else {
            String::new()
        };

        // Try to get cached data first
        if let Some(cache_manager) = &self.cache_manager {
            status.update_message("Checking cache...");

            if let Some(cached_repos) = cache_manager.get_cached_repositories(
                username,
                limit,
                topic_stargazer_count_limit,
                include_private,
            )? {
                println!("Using fully cached data - {} repositories found!", cached_repos.len());
                return Ok(cached_repos);
            }

            // Check for partial cache - see what pages we can reuse
            if let Some((next_page_to_fetch, cursor)) = cache_manager.get_next_page_to_fetch(&cache_key)? {
                status.update_message(&format!("Found partial cache, resuming from page {}", next_page_to_fetch));
                return self.fetch_repositories_with_partial_cache(
                    username,
                    limit,
                    topic_stargazer_count_limit,
                    include_private,
                    next_page_to_fetch,
                    cursor,
                    status,
                ).await;
            }

            // Clean up expired pages before starting fresh
            cache_manager.clear_expired_pages(&cache_key)?;
        }

        // Make full API request
        status.update_message(&format!("Fetching starred repositories for user: {}", username));
        self.fetch_repositories_from_api(
            username,
            limit,
            topic_stargazer_count_limit,
            include_private,
            status,
        ).await
    }

    /// Fetch repositories with partial cache support
    async fn fetch_repositories_with_partial_cache(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_stargazer_count_limit: i32,
        include_private: bool,
        start_page: usize,
        start_cursor: Option<String>,
        status: &mut StatusIndicator,
    ) -> Result<Vec<Repository>, Box<StarredError>> {
        let cache_key = if let Some(cache_manager) = &self.cache_manager {
            cache_manager.generate_cache_key(username, limit, topic_stargazer_count_limit, include_private)
        } else {
            String::new()
        };

        // First, collect cached repositories
        let mut all_repositories = Vec::new();
        let mut total_collected = 0;

        // Get cached pages before the start page
        if let Some(cache_manager) = &self.cache_manager {
            for page_number in 1..(start_page) {
                if let Some(page_data) = cache_manager.read_page_data(&cache_key, page_number)? {
                    for repo in page_data.repositories {
                        if let Some(limit) = limit {
                            if total_collected >= limit {
                                return Ok(all_repositories);
                            }
                        }
                        all_repositories.push(repo);
                        total_collected += 1;
                    }
                }
            }
        }

        // Fetch remaining pages
        let additional_repos = self.fetch_repositories_from_page(
            username,
            limit.map(|l| l.saturating_sub(total_collected)),
            topic_stargazer_count_limit,
            include_private,
            start_page,
            start_cursor,
            status,
        ).await?;

        all_repositories.extend(additional_repos);
        Ok(all_repositories)
    }

    async fn fetch_repositories_from_api(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_stargazer_count_limit: i32,
        include_private: bool,
        status: &StatusIndicator,
    ) -> Result<Vec<Repository>, Box<StarredError>> {
        self.fetch_repositories_from_page(
            username,
            limit,
            topic_stargazer_count_limit,
            include_private,
            1,
            None,
            status,
        ).await
    }

    /// Fetch repositories starting from a specific page (used for both full and partial fetching)
    async fn fetch_repositories_from_page(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_stargazer_count_limit: i32,
        include_private: bool,
        start_page: usize,
        start_cursor: Option<String>,
        status: &StatusIndicator,
    ) -> Result<Vec<Repository>, Box<StarredError>> {
        let mut items = Vec::new();
        let mut after: Option<String> = start_cursor;
        let mut total_fetched = 0;
        let mut current_page = start_page;

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

            // Process repositories for this page
            let mut page_repositories = Vec::new();

            for repo in &starred_repos.nodes {
                if let Some(limit_value) = limit {
                    if total_fetched >= limit_value {
                        // Cache the current page before returning
                        if let Some(cache_manager) = &self.cache_manager {
                            if !page_repositories.is_empty() {
                                let _ = cache_manager.cache_page(
                                    username,
                                    limit,
                                    topic_stargazer_count_limit,
                                    include_private,
                                    current_page,
                                    page_repositories,
                                    after.clone(),
                                    starred_repos.page_info.end_cursor.clone(),
                                    starred_repos.page_info.has_next_page,
                                    None, // ETag not applicable for GraphQL POST requests
                                    None, // Rate limit info not available in GraphQL response
                                    None, // Rate limit reset not available in GraphQL response
                                );
                            }
                        }
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

                let repository = Repository {
                    name,
                    description,
                    language,
                    url,
                    is_private,
                    topics,
                };

                page_repositories.push(repository.clone());
                items.push(repository);
                total_fetched += 1;
            }

            // Cache the current page
            if let Some(cache_manager) = &self.cache_manager {
                if !page_repositories.is_empty() {
                    let _ = cache_manager.cache_page(
                        username,
                        limit,
                        topic_stargazer_count_limit,
                        include_private,
                        current_page,
                        page_repositories,
                        after.clone(),
                        starred_repos.page_info.end_cursor.clone(),
                        starred_repos.page_info.has_next_page,
                        None, // ETag not applicable for GraphQL POST requests
                        None, // Rate limit info not available in GraphQL response
                        None, // Rate limit reset not available in GraphQL response
                    );
                }
            }

            status.update_message(&format!(
                "Fetched page {} - {} repositories this page (total: {})",
                current_page,
                starred_repos.nodes.len(),
                total_fetched
            ));

            if starred_repos.page_info.has_next_page {
                after = starred_repos.page_info.end_cursor.clone();
                current_page += 1;
                // Rate limiting - be nice to GitHub's API
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            } else {
                break;
            }
        }

        Ok(items)
    }

    /// Streaming version that yields repositories as they're fetched, without collecting them all in memory
    pub async fn get_user_starred_by_username_streaming<F>(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_stargazer_count_limit: i32,
        include_private: bool,
        status: &mut StatusIndicator,
        mut callback: F,
    ) -> Result<(), Box<StarredError>>
    where
        F: FnMut(Repository) -> Result<(), Box<dyn std::error::Error>>,
    {
        let cache_key = if let Some(cache_manager) = &self.cache_manager {
            cache_manager.generate_cache_key(username, limit, topic_stargazer_count_limit, include_private)
        } else {
            String::new()
        };

        // Try to get cached data first
        if let Some(cache_manager) = &self.cache_manager {
            status.update_message("Checking cache...");

            // Check if we have full cached data
            if let Some(cached_repos) = cache_manager.get_cached_repositories(
                username,
                limit,
                topic_stargazer_count_limit,
                include_private,
            )? {
                status.update_message(&format!("Using fully cached data - {} repositories found!", cached_repos.len()));
                for repo in cached_repos {
                    if let Err(e) = callback(repo) {
                        return Err(Box::new(StarredError {
                            message: format!("Error processing repository: {}", e),
                        }));
                    }
                }
                return Ok(());
            }

            // Check for partial cache - process cached pages first, then fetch remaining
            if let Some((next_page_to_fetch, cursor)) = cache_manager.get_next_page_to_fetch(&cache_key)? {
                status.update_message(&format!("Found partial cache, processing cached pages first..."));

                // Stream cached pages
                let mut total_processed = 0;
                for page_number in 1..next_page_to_fetch {
                    if let Some(page_data) = cache_manager.read_page_data(&cache_key, page_number)? {
                        for repo in page_data.repositories {
                            if let Some(limit_val) = limit {
                                if total_processed >= limit_val {
                                    return Ok(());
                                }
                            }
                            if let Err(e) = callback(repo) {
                                return Err(Box::new(StarredError {
                                    message: format!("Error processing cached repository: {}", e),
                                }));
                            }
                            total_processed += 1;
                        }
                    }
                }

                // Now stream the remaining pages from API
                status.update_message(&format!("Resuming from page {}", next_page_to_fetch));
                return self.fetch_repositories_streaming_from_page(
                    username,
                    limit.map(|l| l.saturating_sub(total_processed)),
                    topic_stargazer_count_limit,
                    include_private,
                    next_page_to_fetch,
                    cursor,
                    status,
                    callback,
                ).await;
            }

            // Clean up expired pages before starting fresh
            cache_manager.clear_expired_pages(&cache_key)?;
        }

        // Make full API request with streaming
        status.update_message(&format!("Fetching starred repositories for user: {}", username));
        self.fetch_repositories_streaming_from_page(
            username,
            limit,
            topic_stargazer_count_limit,
            include_private,
            1,
            None,
            status,
            callback,
        ).await
    }

    /// Streaming version of fetch_repositories_from_page that yields repositories as they're processed
    async fn fetch_repositories_streaming_from_page<F>(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_stargazer_count_limit: i32,
        include_private: bool,
        start_page: usize,
        start_cursor: Option<String>,
        status: &StatusIndicator,
        mut callback: F,
    ) -> Result<(), Box<StarredError>>
    where
        F: FnMut(Repository) -> Result<(), Box<dyn std::error::Error>>,
    {
        let mut after: Option<String> = start_cursor;
        let mut total_fetched = 0;
        let mut current_page = start_page;

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

            // Process repositories for this page and cache them
            let mut page_repositories = Vec::new();

            for repo in &starred_repos.nodes {
                if let Some(limit_value) = limit {
                    if total_fetched >= limit_value {
                        // Cache the current page before returning
                        if let Some(cache_manager) = &self.cache_manager {
                            if !page_repositories.is_empty() {
                                let _ = cache_manager.cache_page(
                                    username,
                                    limit,
                                    topic_stargazer_count_limit,
                                    include_private,
                                    current_page,
                                    page_repositories,
                                    after.clone(),
                                    starred_repos.page_info.end_cursor.clone(),
                                    starred_repos.page_info.has_next_page,
                                    None,
                                    None,
                                    None,
                                );
                            }
                        }
                        return Ok(());
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

                let repository = Repository {
                    name,
                    description,
                    language,
                    url,
                    is_private,
                    topics,
                };

                // Cache this repository for later
                page_repositories.push(repository.clone());

                // Immediately yield this repository to the callback
                if let Err(e) = callback(repository) {
                    return Err(Box::new(StarredError {
                        message: format!("Error processing repository: {}", e),
                    }));
                }

                total_fetched += 1;
            }

            // Cache the current page
            if let Some(cache_manager) = &self.cache_manager {
                if !page_repositories.is_empty() {
                    let _ = cache_manager.cache_page(
                        username,
                        limit,
                        topic_stargazer_count_limit,
                        include_private,
                        current_page,
                        page_repositories,
                        after.clone(),
                        starred_repos.page_info.end_cursor.clone(),
                        starred_repos.page_info.has_next_page,
                        None,
                        None,
                        None,
                    );
                }
            }

            status.update_message(&format!(
                "Processed page {} - {} repositories this page (total: {})",
                current_page,
                starred_repos.nodes.len(),
                total_fetched
            ));

            if starred_repos.page_info.has_next_page {
                after = starred_repos.page_info.end_cursor.clone();
                current_page += 1;
                // Rate limiting - be nice to GitHub's API
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            } else {
                break;
            }
        }

        Ok(())
    }
}
