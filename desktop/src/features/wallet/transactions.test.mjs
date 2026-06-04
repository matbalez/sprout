import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  formatWalletTransactionTitle,
  walletTransactionNotes,
} from "@/features/wallet/transactions";

function transaction(overrides = {}) {
  return {
    id: "payment-1",
    rail: "offer",
    kind: "offer",
    direction: "inbound",
    status: "completed",
    statusMessage: "",
    amountSats: 10,
    feesSats: 0,
    message: null,
    personalNote: null,
    createdAtMs: 0,
    updatedAtMs: 0,
    agentPayment: null,
    ...overrides,
  };
}

describe("wallet transaction display", () => {
  it("uses BOLT12 language for inbound and outbound offer payments", () => {
    assert.equal(
      formatWalletTransactionTitle(transaction({ direction: "inbound" })),
      "incoming BOLT12 payment",
    );
    assert.equal(
      formatWalletTransactionTitle(transaction({ direction: "outbound" })),
      "outgoing BOLT12 payment",
    );
  });

  it("shows payer and personal notes without blank or duplicate values", () => {
    assert.deepEqual(
      walletTransactionNotes(
        transaction({
          message: "  lunch reimbursement  ",
          personalNote: "lunch reimbursement",
        }),
      ),
      ["lunch reimbursement"],
    );

    assert.deepEqual(
      walletTransactionNotes(
        transaction({
          message: "thanks for the help",
          personalNote: "Sprout profile payment",
        }),
      ),
      ["thanks for the help", "Sprout profile payment"],
    );
  });

  it("does not expose Sprout tip correlation notes as raw payer messages", () => {
    assert.deepEqual(
      walletTransactionNotes(
        transaction({
          message:
            "sprout-tip:v1:1069491accdc43a6bbfb2d9f8b4d0afb:dc91b7ef91fa438a4c8d8904c55113d65e11db006bff3c045568a965514ceedd:00000000000040008000000000000000:10",
        }),
      ),
      ["Sprout message tip"],
    );
  });

  it("shows agent payment annotations before wallet notes", () => {
    const annotation = {
      paymentId: "payment-1",
      agentPubkey: "a".repeat(64),
      agentName: "Supercoder",
      protocol: "L402",
      endpoint: "https://api.example.com/paid",
      endpointHost: "api.example.com",
      endpointPath: "/paid",
      consentEventId: "b".repeat(64),
      status: "completed",
      statusMessage: "paid",
      amountSats: 21,
      feesSats: 1,
      paymentHash: "c".repeat(64),
      createdAtMs: 0,
      updatedAtMs: 1,
    };

    assert.equal(
      formatWalletTransactionTitle(transaction({ agentPayment: annotation })),
      "agent L402 payment",
    );

    assert.deepEqual(
      walletTransactionNotes(
        transaction({
          agentPayment: annotation,
          personalNote: "Sprout agent L402 payment",
        }),
      ),
      [
        "Agent: Supercoder · L402: api.example.com/paid",
        "Sprout agent L402 payment",
      ],
    );
  });
});
