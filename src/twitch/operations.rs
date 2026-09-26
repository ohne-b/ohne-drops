use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub enum Operation {
    Inventory,
    Campaigns,
    CampaignDetails,
    GameDirectory,
    StreamInfo,
    CurrentDrop,
    ClaimDrop,
    AvailableDrops,
    DeleteNotification,
}

impl Operation {
    pub fn request(self, variables: Value) -> Value {
        let (name, hash) = match self {
            Self::Inventory => (
                "Inventory",
                "d86775d0ef16a63a33ad52e80eaff963b2d5b72fada7c991504a57496e1d8e4b",
            ),
            Self::Campaigns => (
                "ViewerDropsDashboard",
                "5a4da2ab3d5b47c9f9ce864e727b2cb346af1e3ea8b897fe8f704a97ff017619",
            ),
            Self::CampaignDetails => (
                "DropCampaignDetails",
                "039277bf98f3130929262cc7c6efd9c141ca3749cb6dca442fc8ead9a53f77c1",
            ),
            Self::GameDirectory => (
                "DirectoryPage_Game",
                "cb5dc816e139dcb8a118f14b4b677d59abc224a4b016c4bc2bb00a47fe0ddec4",
            ),
            Self::StreamInfo => (
                "VideoPlayerStreamInfoOverlayChannel",
                "198492e0857f6aedead9665c81c5a06d67b25b58034649687124083ff288597d",
            ),
            Self::CurrentDrop => (
                "DropCurrentSessionContext",
                "4d06b702d25d652afb9ef835d2a550031f1cf762b193523a92166f40ea3d142b",
            ),
            Self::ClaimDrop => (
                "DropsPage_ClaimDropRewards",
                "a455deea71bdc9015b78eb49f4acfbce8baa7ccbedd28e549bb025bd0f751930",
            ),
            Self::AvailableDrops => (
                "DropsHighlightService_AvailableDrops",
                "9a62a09bce5b53e26e64a671e530bc599cb6aab1e5ba3cbd5d85966d3940716f",
            ),
            Self::DeleteNotification => (
                "OnsiteNotifications_DeleteNotification",
                "13d463c831f28ffe17dccf55b3148ed8b3edbbd0ebadd56352f1ff0160616816",
            ),
        };
        json!({"operationName":name,"variables":variables,"extensions":{"persistedQuery":{"version":1,"sha256Hash":hash}}})
    }
}

pub fn directory(slug: &str, limit: usize) -> Value {
    Operation::GameDirectory.request(json!({"limit":limit,"slug":slug,"imageWidth":50,"includeCostreaming":false,
        "options":{"broadcasterLanguages":[],"freeformTags":null,"includeRestricted":["SUB_ONLY_LIVE"],"recommendationsContext":{"platform":"web"},
            "sort":"RELEVANCE","systemFilters":["DROPS_ENABLED"],"tags":[],"requestID":"JIRA-VXP-2397"},"sortTypeIsRecency":false}))
}
