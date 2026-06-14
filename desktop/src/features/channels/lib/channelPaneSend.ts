export type ChannelPaneMessageSendOptions = {
  bountyAmountSats?: number | null;
  kudos?: boolean;
};

type SendChannelPaneMessageArgs = {
  completeWelcomeComposerBanner: () => void;
  content: string;
  mediaTags?: string[][];
  mentionPubkeys: string[];
  onSendMessage: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
    options?: ChannelPaneMessageSendOptions,
  ) => Promise<void>;
  options?: ChannelPaneMessageSendOptions;
  shouldCompleteWelcomeBanner: boolean;
};

export async function sendChannelPaneMessage({
  completeWelcomeComposerBanner,
  content,
  mediaTags,
  mentionPubkeys,
  onSendMessage,
  options,
  shouldCompleteWelcomeBanner,
}: SendChannelPaneMessageArgs) {
  await onSendMessage(content, mentionPubkeys, mediaTags, options);

  if (shouldCompleteWelcomeBanner) {
    completeWelcomeComposerBanner();
  }
}
