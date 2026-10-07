-- P2b (docs/specs/agent-mcp.md §4, IMPLEMENTING §17): the staged-changes
-- queue. Agents never write here: the agent process drops a file in
-- <local data>/agent/inbox/, and the app checks it and stores the result,
-- staged for approval or rejected with the reason. Approving runs the
-- kind's normal app code; nothing here acts by itself.
CREATE TABLE proposals (
    id            INTEGER PRIMARY KEY,
    flavor        TEXT NOT NULL,
    file          TEXT NOT NULL UNIQUE,   -- the inbox file's name: a file is stored once
    kind          TEXT NOT NULL,          -- login_note (P2b); quest_plan, list later
    body          TEXT,                   -- the checked body as JSON: what the preview shows and Approve applies
    producer      TEXT NOT NULL,          -- the client's own name for itself: a claim
    reason        TEXT,                   -- the agent's reason, if it gave one
    created_at    INTEGER NOT NULL,       -- when the agent proposed it
    received_at   INTEGER NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('staged', 'applied', 'discarded', 'rejected')),
    status_reason TEXT,                   -- rejected: why, in plain words
    decided_at    INTEGER
) STRICT;

CREATE INDEX proposals_status ON proposals (flavor, status, created_at);

-- An approved agent note says who proposed it ("from "Claude Desktop", approved").
ALTER TABLE login_notes ADD COLUMN producer TEXT;
