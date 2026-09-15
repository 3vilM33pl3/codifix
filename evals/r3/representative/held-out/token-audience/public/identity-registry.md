# Stipulated identities and policy

Issuer I and signing key K are trusted by both services. All listed tokens have
valid signatures under K and validity 09:00 through 11:00 on the reference day.
Token labels are synthetic IDs, not credential bytes.

| Token | Principal | Audience | Scope |
| --- | --- | --- | --- |
| t1 | alex | rowan-gateway | inventory.read |
| t2 | alex | rowan-inventory | inventory.read |
| t3 | alex | rowan-inventory-admin | inventory.read |
| t4 | alex | [rowan-gateway,rowan-inventory] | inventory.read |
| t5 | alex | rowan-inventory | inventory.write |

At 10:00 alex is a member of project lab, not project game. Membership of lab
does not grant game access. There is no administrator bypass in A10. At 10:05
the membership authority removes alex from lab. That does not change the token
signatures or their 11:00 expiry; resource authorization must consult current
membership. These facts do not describe actual employee identities or projects.
