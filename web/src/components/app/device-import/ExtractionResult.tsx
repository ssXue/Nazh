import type { ExtractionProposal } from '../../../hooks/use-device-assets';
import { FileYamlIcon } from '../AppIcons';

export function ExtractionResult({
  yaml,
  proposal,
}: {
  yaml: string;
  proposal: ExtractionProposal | null;
}) {
  return (
    <div className="dm-drawer__result">
      <div className="dm-drawer__result-header">
        <FileYamlIcon width={14} height={14} />
        <span>抽取结果</span>
      </div>
      <pre className="dm-drawer__result-yaml">{yaml}</pre>

      {proposal?.capabilityYamls.length ? (
        <details className="dm-drawer__capabilities">
          <summary>推断能力 ({proposal.capabilityYamls.length})</summary>
          {proposal.capabilityYamls.map((cap, idx) => (
            <pre key={idx} className="dm-drawer__result-yaml dm-drawer__result-yaml--small">{cap}</pre>
          ))}
        </details>
      ) : null}

      {proposal?.uncertainties.length ? (
        <div className="dm-drawer__uncertainties">
          <h4>待确认项 ({proposal.uncertainties.length})</h4>
          <ul>
            {proposal.uncertainties.map((u, idx) => (
              <li key={idx}>
                <code>{u.fieldPath}</code>：{u.guessedValue}
                <span className="dm-drawer__reason">{u.reason}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {proposal?.warnings.length ? (
        <div className="dm-drawer__warnings">
          <h4>警告 ({proposal.warnings.length})</h4>
          <ul>
            {proposal.warnings.map((w, idx) => (
              <li key={idx}>{w}</li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
