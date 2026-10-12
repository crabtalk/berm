//! What a desk covers.

/// A beat a desk covers.
pub struct Topic {
    /// `supply_chain`.
    pub name: &'static str,
    /// `Supply chain`.
    pub label: &'static str,
    pub description: &'static str,
}

pub const ALL: &[Topic] = &[
    AI,
    TECH,
    SECURITY,
    SCIENCE,
    HEALTH,
    CLIMATE,
    BUSINESS,
    FINANCE,
    CRYPTO,
    RETAIL,
    FOOD,
    SUPPLY_CHAIN,
    MARKETING,
    DESIGN,
    GAMING,
    CULTURE,
    WORLD,
    POLICY,
    SPORTS,
];

pub const AI: Topic = Topic {
    name: "ai",
    label: "AI",
    description: "Artificial intelligence: models, the labs building them, and the products they reach.",
};

pub const TECH: Topic = Topic {
    name: "tech",
    label: "Technology",
    description: "Technology at large — devices, platforms, software, infrastructure, and the companies shipping them.",
};

pub const SECURITY: Topic = Topic {
    name: "security",
    label: "Security",
    description: "Breaches, vulnerabilities, ransomware, surveillance, and the defenses against them.",
};

pub const SCIENCE: Topic = Topic {
    name: "science",
    label: "Science",
    description: "Research and discovery: physics, biology, space, mathematics, medicine as science rather than as care.",
};

pub const HEALTH: Topic = Topic {
    name: "health",
    label: "Health",
    description: "Medicine, biotech, pharma, clinical research, and public health.",
};

pub const CLIMATE: Topic = Topic {
    name: "climate",
    label: "Climate",
    description: "Climate, energy, and the transition: emissions, grids, renewables, electrification, and electric transport.",
};

pub const BUSINESS: Topic = Topic {
    name: "business",
    label: "Business",
    description: "Companies and the economy: earnings, executives, strategy, deals, startups, and the workplace.",
};

pub const FINANCE: Topic = Topic {
    name: "finance",
    label: "Finance",
    description: "Markets and money: equities, bonds, currencies, commodities, banks, and the macro moving them.",
};

pub const CRYPTO: Topic = Topic {
    name: "crypto",
    label: "Crypto",
    description: "Cryptocurrency and blockchain: protocols, exchanges, tokens, and their regulation.",
};

pub const RETAIL: Topic = Topic {
    name: "retail",
    label: "Retail",
    description: "Selling to consumers: chains, ecommerce, marketplaces, brands, and store operations.",
};

pub const FOOD: Topic = Topic {
    name: "food",
    label: "Food",
    description: "Food and drink as an industry and a culture: manufacturers, grocery, restaurants, and how people eat.",
};

pub const SUPPLY_CHAIN: Topic = Topic {
    name: "supply_chain",
    label: "Supply chain",
    description: "Sourcing, freight, warehousing, packaging, procurement, and tariffs — how a product reaches the shelf.",
};

pub const MARKETING: Topic = Topic {
    name: "marketing",
    label: "Marketing",
    description: "Marketing, advertising, and the media business: campaigns, agencies, channels, and how publishers make money.",
};

pub const DESIGN: Topic = Topic {
    name: "design",
    label: "Design",
    description: "Design practice and its output: architecture, interiors, products, graphics, branding, and the tools for them.",
};

pub const GAMING: Topic = Topic {
    name: "gaming",
    label: "Gaming",
    description: "Video games: releases, reviews, the studios making them, and the craft of making them.",
};

pub const CULTURE: Topic = Topic {
    name: "culture",
    label: "Culture",
    description: "Film, television, music, and the fandoms and industries around them.",
};

pub const WORLD: Topic = Topic {
    name: "world",
    label: "World",
    description: "General and international news — the front page, wherever the newsroom sits.",
};

pub const POLICY: Topic = Topic {
    name: "policy",
    label: "Policy",
    description: "Government, regulation, courts, and the professions shaped by them.",
};

pub const SPORTS: Topic = Topic {
    name: "sports",
    label: "Sports",
    description: "The business of sport: leagues, media rights, sponsorship, and franchise economics.",
};
