-- Cross-instance revenue sharing for index-only federation.
--
-- When a viewer arrives at this instance via a signed referral from another
-- PPV Stream instance (the "traffic provider"), a configurable percentage of
-- any resulting purchase is credited to the referring instance.
--
-- Revenue is tracked in integer minor units (cents) using basis points
-- (1 bp = 0.01%).  All arithmetic uses integer division; remainders are
-- dropped (floor semantics).

-- federation_referrals
-- Records every signed referral token presented to this instance.
-- `verified` is set to TRUE only when the RSA signature can be validated
-- against the referring actor's public key at the time of presentation.
CREATE TABLE IF NOT EXISTS federation_referrals (
    id UUID PRIMARY KEY,
    referring_domain TEXT NOT NULL,
    raw_payload TEXT NOT NULL,
    viewer_nonce TEXT,
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_federation_referrals_domain
    ON federation_referrals (referring_domain, verified);

-- revenue_share_policies
-- Defines how much a traffic provider earns per successful payment.
-- Absent a row for a domain, the referring instance receives no share.
-- share_basis_points is capped at 5 000 (50 %) to prevent misconfiguration.
CREATE TABLE IF NOT EXISTS revenue_share_policies (
    id UUID PRIMARY KEY,
    instance_domain TEXT NOT NULL UNIQUE,
    share_basis_points INTEGER NOT NULL DEFAULT 500,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (share_basis_points >= 0 AND share_basis_points <= 5000)
);

-- federation_revenue_shares
-- One row per payment that has an active referral attribution.
-- UNIQUE (invoice_id, invoice_type) enforces idempotency — processing
-- the same invoice twice yields the same row.
CREATE TABLE IF NOT EXISTS federation_revenue_shares (
    id UUID PRIMARY KEY,
    invoice_id TEXT NOT NULL,
    invoice_type TEXT NOT NULL,
    referral_id UUID REFERENCES federation_referrals(id),
    referring_domain TEXT,
    gross_cents BIGINT NOT NULL,
    share_basis_points INTEGER NOT NULL,
    share_cents BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (invoice_id, invoice_type),
    CHECK (status IN ('pending', 'settled', 'reversed')),
    CHECK (gross_cents >= 0),
    CHECK (share_cents >= 0)
);

CREATE INDEX IF NOT EXISTS idx_federation_revenue_shares_domain
    ON federation_revenue_shares (referring_domain, status);

-- revenue_ledger_entries
-- Immutable double-entry ledger.  Each revenue share record may have
-- multiple ledger lines (initial credit, later reversal, etc.).
CREATE TABLE IF NOT EXISTS revenue_ledger_entries (
    id UUID PRIMARY KEY,
    revenue_share_id UUID NOT NULL REFERENCES federation_revenue_shares(id),
    entry_type TEXT NOT NULL,
    amount_cents BIGINT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (entry_type IN ('credit', 'debit', 'refund', 'chargeback')),
    CHECK (amount_cents >= 0)
);

CREATE INDEX IF NOT EXISTS idx_revenue_ledger_share
    ON revenue_ledger_entries (revenue_share_id);
