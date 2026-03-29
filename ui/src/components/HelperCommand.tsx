import { HELPER_COMMAND } from "../lib/inspect";

type HelperCommandProps = {
  onCopy: () => void;
};

export function HelperCommand({ onCopy }: HelperCommandProps) {
  return (
    <div className="helper-command" aria-label="Local helper command">
      <span className="helper-command-label">Local helper</span>
      <code>{HELPER_COMMAND}</code>
      <button onClick={onCopy}>Copy</button>
    </div>
  );
}
