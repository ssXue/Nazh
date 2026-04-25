import type { NodeSettingsProps } from '../settings-shared';

export function SwitchNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <section className="flowgram-panel flowgram-panel--branches">
      <div className="flowgram-panel__header">
        <h4>分支设置</h4>
      </div>

      <div className="flowgram-branch-editor">
        {draft.branches.map((branch, index) => (
          <div key={`${branch.key}:${index}`} className="flowgram-branch-editor__row">
            <input
              value={branch.key}
              onChange={(event) => {
                const nextBranches = draft.branches.map((item, itemIndex) =>
                  itemIndex === index
                    ? { ...item, key: event.target.value }
                    : item,
                );
                updateDraft({ branches: nextBranches });
              }}
              placeholder="branch_key"
            />
            <input
              value={branch.label}
              onChange={(event) => {
                const nextBranches = draft.branches.map((item, itemIndex) =>
                  itemIndex === index
                    ? { ...item, label: event.target.value }
                    : item,
                );
                updateDraft({ branches: nextBranches });
              }}
              placeholder="显示名称"
            />
            <button
              type="button"
              className="ghost"
              onClick={() =>
                updateDraft({
                  branches: draft.branches.filter((_, itemIndex) => itemIndex !== index),
                })
              }
            >
              删除
            </button>
          </div>
        ))}

        <button
          type="button"
          className="ghost"
          onClick={() =>
            updateDraft({
              branches: [
                ...draft.branches,
                {
                  key: `branch_${draft.branches.length + 1}`,
                  label: `Branch ${draft.branches.length + 1}`,
                },
              ],
            })
          }
        >
          添加分支
        </button>
      </div>
    </section>
  );
}
