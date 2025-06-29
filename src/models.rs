use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub description: String,
    pub language: String,
    pub url: String,
    pub is_private: bool,
    pub topics: Vec<String>,
}

#[derive(Deserialize)]
pub struct GraphQLResponse {
    pub data: Data,
}

#[derive(Deserialize)]
pub struct Data {
    pub user: User,
}

#[derive(Deserialize)]
pub struct User {
    #[serde(rename = "starredRepositories")]
    pub starred_repositories: StarredRepositories,
}

#[derive(Deserialize)]
pub struct StarredRepositories {
    pub nodes: Vec<RepoNode>,
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
}

#[derive(Deserialize)]
pub struct RepoNode {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
    pub description: Option<String>,
    pub url: String,
    #[serde(rename = "isPrivate")]
    pub is_private: bool,
    pub languages: Languages,
    #[serde(rename = "repositoryTopics")]
    pub repository_topics: RepositoryTopics,
}

#[derive(Deserialize)]
pub struct Languages {
    pub edges: Vec<LanguageEdge>,
}

#[derive(Deserialize)]
pub struct LanguageEdge {
    pub node: LanguageNode,
}

#[derive(Deserialize)]
pub struct LanguageNode {
    pub name: String,
}

#[derive(Deserialize)]
pub struct RepositoryTopics {
    pub nodes: Vec<TopicNode>,
}

#[derive(Deserialize)]
pub struct TopicNode {
    pub topic: Topic,
}

#[derive(Deserialize)]
pub struct Topic {
    pub name: String,
    #[serde(rename = "stargazerCount")]
    pub stargazer_count: i32,
}

#[derive(Deserialize)]
pub struct PageInfo {
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
}
