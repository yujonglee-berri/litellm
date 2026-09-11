pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 4002;

pub const PORTING_IDENTITY_ROUTES: &[&str] = &[
    "/user/new",
    "/user/update",
    "/user/delete",
    "/user/info",
    "/user/list",
    "/team/new",
    "/team/update",
    "/team/delete",
    "/team/info",
    "/team/list",
    "/organization/new",
    "/organization/update",
    "/organization/delete",
    "/organization/info",
    "/organization/list",
    "/customer/new",
    "/customer/update",
    "/customer/delete",
    "/customer/info",
    "/customer/list",
    "/scim/v2/*path",
];

pub const PORTING_MODEL_AND_ROUTING_ROUTES: &[&str] = &[
    "/model/new",
    "/model/update",
    "/model/delete",
    "/model/info",
    "/model_group/info",
    "/router/settings",
    "/auto_router/*path",
    "/access_group/*path",
    "/v1/access_group/*path",
];

pub const PORTING_BUDGET_AND_USAGE_ROUTES: &[&str] = &[
    "/budget/*path",
    "/spend/*path",
    "/global/spend/*path",
    "/usage/*path",
    "/management/v1/budgets",
    "/management/v1/spend/logs",
];

pub const PORTING_CONFIGURATION_ROUTES: &[&str] = &[
    "/config/*path",
    "/config_overrides/*path",
    "/cache/*path",
    "/fallback/*path",
    "/callbacks/*path",
];

pub const PORTING_ACCESS_POLICY_ROUTES: &[&str] = &[
    "/jwt/key/mapping/*path",
    "/guardrails/*path",
    "/v2/guardrails/*path",
    "/policy/*path",
    "/policies/*path",
    "/compliance/*path",
];

pub const PORTING_TOOL_AND_AGENT_MANAGEMENT_ROUTES: &[&str] = &[
    "/v1/mcp/server",
    "/v1/mcp/server/*path",
    "/v1/tool/*path",
    "/search_tools/*path",
    "/v1/a2a/discover",
    "/v1/agents",
    "/v1/agents/*path",
    "/agent/daily/activity",
];

pub const PORTING_MANAGEMENT_ROUTE_GROUPS: &[&[&str]] = &[
    PORTING_IDENTITY_ROUTES,
    PORTING_MODEL_AND_ROUTING_ROUTES,
    PORTING_BUDGET_AND_USAGE_ROUTES,
    PORTING_CONFIGURATION_ROUTES,
    PORTING_ACCESS_POLICY_ROUTES,
    PORTING_TOOL_AND_AGENT_MANAGEMENT_ROUTES,
];
