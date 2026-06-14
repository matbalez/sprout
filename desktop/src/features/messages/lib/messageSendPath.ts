export function shouldUseValidatedMessageSendPath({
  annotationTags = [],
  emojiTags = [],
  mentionTags = [],
  mediaTags = [],
  paidPostReceiptEventId = null,
  parentEventId = null,
}: {
  annotationTags?: readonly string[][];
  emojiTags?: readonly string[][];
  mentionTags?: readonly string[][];
  mediaTags?: readonly string[][];
  paidPostReceiptEventId?: string | null;
  parentEventId?: string | null;
}) {
  return (
    Boolean(parentEventId) ||
    mediaTags.length > 0 ||
    emojiTags.length > 0 ||
    annotationTags.length > 0 ||
    mentionTags.length > 0 ||
    paidPostReceiptEventId !== null
  );
}
