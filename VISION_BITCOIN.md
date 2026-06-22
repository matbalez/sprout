# ⚡ Buzz Bitcoin — Value moves with the work

> A maintainer gives an agent a small budget to investigate a bug. The agent pays
> for a private test dataset, asks a specialist agent for a second opinion, and
> posts the result in the branch channel. The maintainer approves the fix. A
> workflow pays the contributor. A teammate tips the explanation that saved
> everyone an hour. One recipient is asleep; their wallet receives anyway.
>
> No platform credits. No custodial Buzz balance. No copy-pasted invoices. Bitcoin
> moves through the same collaboration context as messages, code, agents, and work.

Collaboration already moves information, attention, access, and effort. Value is
part of that exchange, but today's collaboration tools push it into a payment app,
billing system, marketplace, bank account, or proprietary platform ledger.

Buzz should make transmitting value a native part of building together.

Bitcoin is internet-native digital money implemented as an open protocol. It does
not require Buzz to issue a token, run a closed ledger, or make users dependent on
a payment platform. Open-source money belongs beside open-source software,
open-source communications, and open-source intelligence.

The goal:

**Every Buzz user should be able to hold, send, and receive Bitcoin without giving
custody to Buzz — and humans and agents should be able to use value as naturally as
they use messages, reactions, tools, and workflows.**

---

## Value as a Product Primitive

Once every Buzz user is bestowed a bitcoin Lightning wallet, value can flow in the product experience in all kinds of new and intresting ways.

### Appreciation and attention

- Tip a message, person, agent, project, patch, or document.
- Add sats to kudos without replacing its social meaning.
- Attach a reward to a question or request.
- Fund a bounty directly from an issue or branch channel.
- Let a user publish a price for priority attention or specialized work.
- Let an opt-in channel require a small payment for high-volume unsolicited actions.

### Access to resources

- Pay to unlock a private file, dataset, document, or channel.
- Pay per model inference, API request, search, build minute, or compute job.
- Let an agent buy a tool call or data source while completing a task.
- Sell access to a research feed or specialized agent.
- Pay for a temporary capability instead of creating another subscription account.

A payment receipt can satisfy an access policy. The relay coordinates the action;
it does not custody the money.

### Build and earn together

- Pay contributors when a patch is approved or a release ships.
- Split revenue among people, agents, maintainers, and project funds.
- Let workflows turn approved outcomes into payouts.
- Let projects earn from artifacts, support, data, compute, or services.
- Give humans and agents persistent economic identities and earning histories.

The long-term result is a workspace where people and agents discover work, perform
it, prove it, approve it, and get paid in the same shared context.

---

## Prototype

A prototype Buzz client was created that integrates a lightning wallet that bring to life many of the feature ideas above.

Video walkthroughs of the prototype client:

- [Bitcoin in Buzz (née Sprout) — initial feature exploration](https://www.loom.com/share/f9323cfde3a7419ab82a8efdfa5282f3)
- [Bitcoin in Buzz — hive channels](https://www.loom.com/share/ebfcf339dcb2431aa3a40ba242145437)

---

## BOLT12: The Right Lightning Payment Protocol

Early Lightning applications were built around BOLT 11 invoices and LNURL. They
proved the demand, but they are a poor foundation for Buzz's native experience.

BOLT 11 invoices are single-use payment requests. LNURL and Lightning Addresses add
reusable payment discovery through an HTTP service that returns BOLT 11 invoices.
That works, but it introduces web infrastructure, domain dependencies, and another
service that observes the request.

BOLT 12 moves the negotiation into Lightning itself. A recipient publishes a
reusable **offer**. A payer uses it to request a unique invoice through Lightning
onion messages, then pays it. Blinded paths can hide the recipient's node identity.
Signed, extensible messages provide better proofs and room for protocol evolution.

For Buzz, BOLT 12 is the core payment primitive, giving a person, agent, project, channel, or resource a durable way to receive.

---

## Requirements

The first version of a bitcoin Lightning wallet in Buzz has four hard requirements.

### 1. Every user has a wallet by default

Buzz must be able to programmatically create an embedded wallet during onboarding.
Self-custody is the preferred default where practical, but compatibility is judged
by control, recovery, transparency, and product safety rather than by a marketing
label:

- Buzz and the relay operator cannot custody user funds, extract wallet secrets,
  or unilaterally spend.
- If the wallet exposes user-held recovery material, the user can inspect it in
  wallet settings; it is hidden by default.
- The user can restore, migrate, withdraw, close, or recover as supported by the
  provider without depending on Buzz continuing to exist.
- The wallet supports basic account operations, including reading the current
  balance and listing recent transactions.

A service may host the user's always-online Lightning node or provide other
payment infrastructure. Hosted, federated, or custodial components are compatible
with Buzz only when the trust assumptions are stated plainly, the user has a
practical exit path, and Buzz itself never becomes the custodian.

### 2. BOLT 12 is the core send and receive primitive

A wallet provider must support both sides of the BOLT 12 flow:

- create reusable offers, including amountless offers where appropriate;
- pay offers;
- receive repeated payments to an offer;
- carry payer messages or payment context where supported;
- report payment state and fees reliably.

Buzz features should exchange BOLT 12 offers or provider-neutral references to
them. Provider-specific node IDs, credentials, and API objects stay behind the
wallet adapter.

### 3. Users can send and receive small amounts immediately

A new wallet must be able to receive a small payment without asking the user to
open a channel, find inbound liquidity, wait for manual node setup, or keep the app
open. The provider handles node provisioning, liquidity, routing, and channel
maintenance.

A wallet can send as soon as it has spendable funds. Buzz should be honest about
that boundary: a wallet with a zero balance cannot send. Funding and first receive
should still feel like one onboarding flow rather than a Lightning operations
lesson.

Small payments are the acceptance test. A wallet that works for a large deposit but
cannot economically move a few sats cannot support this vision.

### 4. Users can send to recipients whose clients are offline

The recipient must not need to keep Buzz open.

Their wallet infrastructure must remain able to respond to BOLT 12 invoice
requests, settle incoming payments, persist state safely.

Messages are asynchronous. Value attached to messages must be asynchronous too.

---

## Future requirement: Connecting an Existing Wallet to Buzz

5. **A user can connect an existing wallet and retain BOLT 12 send and receive,
   small-payment support, and receipt while the Buzz client is offline.**


Nostr Wallet Connect, defined by NIP-47, is the established wallet connection
protocol in the Nostr ecosystem. Its current command set centers on BOLT 11
operations such as `pay_invoice` and `make_invoice`; it does not define the BOLT 12
offer operations Buzz needs. It therefore cannot satisfy the complete Buzz wallet
contract today.

Buzz should track NWC's evolution and other work on a BOLT 12-capable wallet
connection protocol. Once a standard is specified, implemented by multiple wallets,
and proven in production, Buzz should adopt it rather than create a permanent
Buzz-specific remote wallet protocol.

The connection standard must support capability discovery, BOLT 12 offer creation
and payment, asynchronous notifications, fee limits, idempotent requests, and
revocable scoped permissions. Connection keys should remain separate from the
user's main Nostr identity.

Buzz should integrate one connection protocol, then gain access to every compatible wallet. It
should not write a bespoke remote integration for every wallet forever.

---

## Wallet Providers, Not a Wallet Vendor

The Buzz product experience should depend on a stable wallet capability layer, not
on one company's SDK.

```text
Humans · Agents · Workflows · Product features
                    |
                    v
          buzz-wallet capability layer
                    |
       +------------+-------------+
       |            |             |
       v            v             v
   Lexe SDK     Future SDK    Wallet connection
   adapter       adapter         protocol
       |            |             |
       +------------+-------------+
                    |
                    v
       User-selected Bitcoin wallet/provider
                    |
                    v
             Lightning Network
```

The capability layer needs provider-neutral operations for provisioning or
connecting a wallet, discovering capabilities, creating and paying offers, quoting
fees, reading balance and payment state, subscribing to payment events, exporting
recovery or exit material where applicable, and closing the wallet safely.

Product code calls those capabilities. It should not care whether the wallet runs
locally, in remotely attested secure hardware, through an embedded SDK, or behind a
future wallet connection protocol.

A relay may advertise supported providers and recommend a default. The client must
identify the provider and its security model clearly. The user can choose another
compatible provider when available, opt out, and later migrate. A relay
recommendation never gives the relay access to wallet credentials.

---

## Sequence

### Phase 1 — Establish the wallet

- Define the provider-neutral capability layer and acceptance suite.
- Select a first embedded wallet SDK to build upon.
- Ship wallet creation, backup, restore, close, and recovery.
- Create and pay BOLT 12 offers.
- Receive while the Buzz client is offline.
- Show balances, payment state, fees, and useful failures.

### Phase 2 — Create product value

- Tip people, agents, and messages.
- Attach rewards and bounties to requests, issues, and branch channels.
- Add payment-gated artifacts, resources, and shared inference.
- Add agent budgets, approval gates, and audit traces.
- Add monetized channels.
- Add channel contributor payouts and revenue splits.

### Phase 3 — Enable wallet choice

- Adopt a BOLT 12-capable wallet connection standard once it is ready and adopted.
- Add more embedded providers through the same adapter boundary.
- Make provider migration and wallet exit routine.

---

## The Point

One relay. One identity model. One event log. Humans, agents, workflows, code, and
project memory already share the same space.

Bitcoin lets value share it too.

Buzz should make money part of collaboration without making Buzz the bank. The
wallet belongs to the user. BOLT 12 is the protocol boundary. Providers are
replaceable. Agents operate under explicit authority. Payments remain connected to
the work that gave them meaning.

Open-source software. Open-source communications. Open-source intelligence.
Open-source money.

Value moves with the work.

---

## References

- [Buzz platform vision](VISION.md)
- [Buzz sovereign vision](VISION_SOVEREIGN.md)
- [Buzz agent vision](VISION_AGENT.md)
- [BOLT 12: Negotiation Protocol for Lightning Payments](https://github.com/lightning/bolts/blob/master/12-offer-encoding.md)
- [NIP-47: Nostr Wallet Connect](https://github.com/nostr-protocol/nips/blob/master/47.md)
- [Lexe SDK](https://github.com/lexe-app/lexe-sdk)
- [Lexe public monorepo and security model](https://github.com/lexe-app/lexe-public)

---

*Buzz 🐝 — value moves with the work.*
