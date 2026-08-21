import { Icon } from "../icons";
import type { ConversationJumpBarProps } from "./type";
import { Button } from "./ui/Button";

export function ConversationJumpBar({
  atTop,
  atBottom,
  unseenCount,
  onJumpTop,
  onJumpBottom,
}: ConversationJumpBarProps) {
  const showBottom = !atBottom || unseenCount > 0;
  const bottomLabel = unseenCount > 0 ? `新增 ${unseenCount} 条事件，回到底部` : "回到底部";

  return (
    <div className="conversation-jump-bar" aria-live="polite">
      {atTop ? null : (
        <Button
          size="sm"
          className="conversation-jump conversation-jump-top"
          aria-label="回到顶部"
          onClick={onJumpTop}
        >
          <span className="conversation-jump-glyph" aria-hidden>
            <Icon name="chevron" size={12} className="conversation-top-icon" />
          </span>
          顶部
        </Button>
      )}
      {showBottom ? (
        <Button
          size="sm"
          className={[
            "conversation-jump",
            "conversation-jump-bottom",
            unseenCount > 0 ? "has-unseen" : "",
          ]
            .filter(Boolean)
            .join(" ")}
          aria-label={bottomLabel}
          onClick={onJumpBottom}
        >
          <span className="conversation-jump-glyph" aria-hidden>
            <Icon name="chevron" size={12} className="conversation-follow-icon" />
          </span>
          底部
          {unseenCount > 0 ? <span className="conversation-jump-count">{unseenCount}</span> : null}
        </Button>
      ) : null}
    </div>
  );
}
