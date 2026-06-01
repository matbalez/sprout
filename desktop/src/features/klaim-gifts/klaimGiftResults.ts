import type { KlaimPayoutResult } from "./api";
import type { KlaimGiftMemberStatus } from "./storage";

export function classifyKlaimPayoutResult(
  result: KlaimPayoutResult,
): KlaimGiftMemberStatus {
  if (result.ok) {
    return "paid";
  }

  const error = result.error?.toLowerCase() ?? "";

  if (result.statusCode === 409) {
    if (error.includes("already been paid")) {
      return "already-paid";
    }
    if (error.includes("max claims")) {
      return "max-claims";
    }
    if (error.includes("in progress") || error.includes("reconciliation")) {
      return "in-progress";
    }
  }

  if (result.statusCode === 422 || result.statusCode === 503) {
    return "retryable";
  }

  if (result.statusCode === 500) {
    return "uncertain";
  }

  return "failed";
}

export function shouldPostKlaimGiftConfirmation(result: KlaimPayoutResult) {
  return result.ok && result.status === "paid";
}

export function klaimPayoutErrorLabel(result: KlaimPayoutResult) {
  if (result.detail) {
    return `${result.error ?? "Klaim payout failed"}: ${result.detail}`;
  }

  return result.error ?? `Klaim payout failed with HTTP ${result.statusCode}`;
}
