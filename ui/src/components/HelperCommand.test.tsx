import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { HelperCommand } from "./HelperCommand";
import { HELPER_COMMAND } from "../lib/inspect";

describe("HelperCommand", () => {
  it("renders the local helper label and container aria-label", () => {
    const onCopy = vi.fn();
    render(<HelperCommand onCopy={onCopy} />);

    expect(screen.getByText("Local helper")).toBeInTheDocument();
    expect(screen.getByLabelText("Local helper command")).toBeInTheDocument();
  });

  it("renders cargo run and copy button", () => {
    const onCopy = vi.fn();
    render(<HelperCommand onCopy={onCopy} />);

    expect(screen.getByText(HELPER_COMMAND)).toBeInTheDocument();
    const copyButton = screen.getByRole("button", { name: "Copy" });
    expect(copyButton).toHaveAttribute("type", "button");
    fireEvent.click(copyButton);
    expect(onCopy).toHaveBeenCalledTimes(1);
  });

  it("renders a custom command and copy button", () => {
    const onCopy = vi.fn();
    const customCommand = "npm test";
    render(<HelperCommand command={customCommand} onCopy={onCopy} />);

    expect(screen.getByText(customCommand)).toBeInTheDocument();
    const copyButton = screen.getByRole("button", { name: "Copy" });
    expect(copyButton).toHaveAttribute("type", "button");
    fireEvent.click(copyButton);
    expect(onCopy).toHaveBeenCalledTimes(1);
  });

  it("renders the command inside a code element", () => {
    const onCopy = vi.fn();
    const command = "custom command";
    render(<HelperCommand command={command} onCopy={onCopy} />);
    const codeElement = screen.getByText(command).closest('code');
    expect(codeElement).toBeInTheDocument();
    expect(codeElement?.textContent).toBe(command);
  });

  it("handles keyboard interaction (Enter/Space) on the code element", () => {
    const onCopy = vi.fn();
    const command = "npm test";
    render(<HelperCommand command={command} onCopy={onCopy} />);
    const codeElement = screen.getByText(command).closest('code');
    if (!codeElement) throw new Error("Code element not found");

    fireEvent.keyDown(codeElement, { key: "Enter", code: "Enter" });
    expect(onCopy).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(codeElement, { key: " ", code: "Space" });
    expect(onCopy).toHaveBeenCalledTimes(2);
  });

  it("renders an empty command and still handles copy", () => {
    const onCopy = vi.fn();
    render(<HelperCommand command="" onCopy={onCopy} />);
    const copyButton = screen.getByRole("button", { name: "Copy" });
    expect(copyButton).toHaveAttribute("type", "button");
    fireEvent.click(copyButton);
    expect(onCopy).toHaveBeenCalledTimes(1);
  });

  it("writes the command text to the clipboard on copy", async () => {
    const onCopy = vi.fn();
    Object.defineProperty(navigator, "clipboard", {
      writable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });

    render(<HelperCommand onCopy={onCopy} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));

    await expect(navigator.clipboard.writeText).toHaveBeenCalledWith(HELPER_COMMAND);
  });
});