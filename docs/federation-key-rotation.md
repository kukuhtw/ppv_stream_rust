# Backup and Key Rotation Guide

## What to back up

| Item | Location | Sensitivity |
|---|---|---|
| `HMAC_SECRET` | Environment variable | **Critical** — losing this makes all encrypted private keys unrecoverable. |
| `FEDERATION_ADMIN_TOKEN` | Environment variable | High — rotate immediately if compromised. |
| `federation_actors` table | PostgreSQL | High — contains encrypted private keys. |
| `federation_domain_rules` table | PostgreSQL | Medium — admin configuration. |
| `revenue_share_policies` table | PostgreSQL | Medium — financial configuration. |
| `revenue_ledger_entries` table | PostgreSQL | High — immutable financial audit trail. |

Standard PostgreSQL database backups (`pg_dump`) cover all table data.
The `HMAC_SECRET` must be backed up separately and stored in a secrets
manager.

---

## Backing up actor private keys

Actor private keys are stored encrypted in `federation_actors.private_key_encrypted`.
To back them up independently:

```sql
COPY (
    SELECT actor_uri, private_key_encrypted
    FROM federation_actors
    WHERE is_local = TRUE
) TO '/tmp/actor_keys_backup.csv' CSV HEADER;
```

Store the backup alongside the `HMAC_SECRET` that was used to encrypt them.

---

## Rotating HMAC_SECRET

Rotating `HMAC_SECRET` requires re-encrypting all local actor private keys.

```
1. Generate a new HMAC_SECRET:
   openssl rand -hex 32

2. For each local actor:
   a. Call decrypt_private_key(encrypted_pem, old_secret) to get the PEM.
   b. Call encrypt_private_key(pem, new_secret) to re-encrypt.
   c. UPDATE federation_actors SET private_key_encrypted = <new_encrypted>
      WHERE actor_uri = <uri>;

3. Deploy the new HMAC_SECRET.
4. Verify the application starts and actor keys decrypt successfully.
```

There is no automated key rotation tool yet; the above steps must be
scripted manually.

---

## Rotating actor RSA keys

To rotate the RSA key pair for a specific local actor:

```
1. Generate new keys:
   Call generate_actor_keys() → { public_key_pem, private_key_pem }

2. Encrypt the new private key:
   Call encrypt_private_key(private_key_pem, HMAC_SECRET)

3. Update the database:
   UPDATE federation_actors
   SET public_key_pem = <new_pem>,
       private_key_encrypted = <new_encrypted>
   WHERE actor_uri = <uri>;

4. Re-publish the actor document:
   The updated actor document at GET /users/:username will now return the
   new public key.  Remote instances will fetch it the next time they
   verify a signature from this actor.
```

> **Note**: there is a brief window after rotation where in-flight
> deliveries signed with the old key may fail verification at the
> recipient.  The recipient will retry; once the new actor document is
> cached at the recipient the retries will succeed.

---

## Rotating FEDERATION_ADMIN_TOKEN

```sh
# Generate a new token
NEW_TOKEN=$(openssl rand -hex 24)

# Update your environment / secrets manager with NEW_TOKEN.
# The token is checked at request time from the environment variable;
# no database update is required.
```

Old sessions using the previous token will be rejected immediately after
the environment variable is updated and the process is restarted (or the
env var is reloaded without restart if your deployment supports it).

---

## Disaster recovery

If `HMAC_SECRET` is lost:

1. Generate new RSA keys for all local actors (rotation procedure above).
2. Update `federation_actors` with the new encrypted private keys.
3. Re-publish actor documents.
4. Ask remote instances to re-fetch the actor documents to pick up the
   new public keys.

Activity history and follow relationships are preserved in the database
and require no action.  Pending delivery jobs signed with the old key
will fail; retry them after the new keys are deployed.
