use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::json;

use crate::error::StarredError;
use crate::models::{GraphQLResponse, Repository};
use crate::status::StatusIndicator;

pub struct GitHubGQL {
    client: reqwest::Client,
    api_url: String,
}

impl GitHubGQL {
    pub fn new(token: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static("starred-rust/0.1.0"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        GitHubGQL {
            client,
            api_url: "https://api.github.com/graphql".to_string(),
        }
    }

    pub async fn get_user_starred_by_username(
        &self,
        username: &str,
        limit: Option<usize>,
        topic_stargazer_count_limit: i32,
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
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            } else {
                break;
            }
        }

        Ok(items)
    }
}
