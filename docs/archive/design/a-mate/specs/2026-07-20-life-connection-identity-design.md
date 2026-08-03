---
status: done
archived: 2026-08-03
---

**Life connection identity design**

**Problem**

- The desktop client previously reused a saved token only when the newly entered server URL exactly matched the saved URL.
- `localhost`, a LAN address, and another hostname can point to the same Life Server, but the client treated them as different servers and registered a second agent and Life.
- Any failed rename request, including a transient network failure, fell through to registration and could create another Life after connectivity recovered.

**Decision**

- Treat the normalized display name entered by the user as the Life identity key on each server.
- `POST /life/register` returns the existing agent and Life with a new token when that name already exists.
- Create a new agent and Life only when the normalized name does not exist on the target server.
- When a saved token exists, validate it against the newly entered URL regardless of the previous URL string.
- If the token is accepted, update the saved URL and reuse the existing agent and Life.
- Register a new identity only when the target server explicitly rejects the token with HTTP 401 or 403.
- Return transient network and server errors to the user instead of registering a replacement identity.
- Do not use `mascot_seed` as the identity key; the user-entered name is authoritative.

**Security tradeoff**

- A user who enters an existing name can receive access to that Life. This is intentional for the current trusted/private Life Server deployment.
- A future public deployment must replace name-only reconnection with account authentication before exposing write access.

**Data cleanup**

- Back up the SQLite volume before deleting an accidental duplicate.
- Delete only a verified empty duplicate Life, its owner agent, and its tokens.
- Restart the Life Server after direct database maintenance so its in-memory state reloads from SQLite.

**Verification**

- Unit-test the distinction between rejected credentials and transient failures.
- Unit-test that registering the same normalized name returns one Life and a usable replacement token.
- Confirm reconnecting through an alias preserves the existing `life_id`.
- Confirm the original designed Life remains available after cleanup.
