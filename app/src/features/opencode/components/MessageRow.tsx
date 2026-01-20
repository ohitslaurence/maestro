import { memo } from "react";
import type { OpenCodeThreadItem } from "../../../types";

type MessageRowProps = {
  item: Extract<OpenCodeThreadItem, { kind: "user-message" | "assistant-message" }>;
};

export const MessageRow = memo(function MessageRow({ item }: MessageRowProps) {
  const isUser = item.kind === "user-message";

  return (
    <div className={`oc-message oc-message--${isUser ? "user" : "assistant"}`}>
      <div className="oc-message__bubble">
        <pre className="oc-message__text">{item.text}</pre>
      </div>
    </div>
  );
});
