import { HELPER_COMMAND } from "../lib/inspect";

type HelperCommandProps = {
  command?: string;
  onCopy: () => void;
};

export function HelperCommand({ command = HELPER_COMMAND, onCopy }: HelperCommandProps) {
  return (
    <div className="helper-command" aria-label="Local helper command">
      <span className="helper-command-label">Local helper</span>
      <code>{command}</code>
      <button onClick={onCopy}>Copy</button>
    </div>
  );
}
