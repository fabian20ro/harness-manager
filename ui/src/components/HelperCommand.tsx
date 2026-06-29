import { useState } from "react";
import { HELPER_COMMAND } from "../lib/inspect";

type HelperCommandProps = {
  command?: string;
  onCopy: () => void;
};

export function HelperCommand({ command = HELPER_COMMAND, onCopy }: HelperCommandProps) {
  const [isCopied, setIsCopied] = useState(false);

  const handleCopy = () => {
    onCopy();
    setIsCopied(true);
    setTimeout(() => setIsCopied(false), 2000);
  };

  return (
    <div className="helper-command" aria-label="Local helper command">
      <span className="helper-command-label">Local helper</span>
      <code aria-hidden="true">{command}</code>
      <button type="button" onClick={handleCopy}>
        {isCopied ? "Copied!" : "Copy"}
      </button>
    </div>
  );
}
