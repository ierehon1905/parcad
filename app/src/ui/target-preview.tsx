/**
 * What the caret's treatment resolves to, drawn over the finished part.
 *
 * Gold, because it is a *source* selection — the same colour the editor
 * underlines the call in. The count is measured: the kernel rebuilds the
 * treatment's child and reports the exact edge set, which is why the panel
 * opens saying it is still resolving rather than guessing from the selector.
 */

import * as S from "../state";
import { Icon } from "./icons";

export function TargetPreview() {
  const preview = S.targetPreview.value;
  if (!preview) return null;

  return (
    <div
      id="target-preview"
      class="absolute top-3.5 left-3.5 px-2.5 py-2 rounded-lg border border-gold-edge
             bg-glass/86 backdrop-blur-lg text-gold font-mono text-tiny pointer-events-none"
    >
      <div class="flex items-center gap-1.5">
        <Icon name="tag" class="size-4 shrink-0 text-ink-dim" />
        <span class="font-sans text-[10px] leading-[1.3] tracking-[0.08em] uppercase text-ink-dim">
          source target
        </span>
      </div>
      <div class="mt-1">.{preview.method}</div>
      <div class="text-ink-dim">{preview.detail}</div>
    </div>
  );
}
