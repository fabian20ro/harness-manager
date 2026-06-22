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

    expect(screen.getByText("cargo run")).toBeInTheDocument();
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

  it("uses the default HELPER_COMMAND when no command is provided", () => {
    const onCopy = vi.fn();
    render(<HelperCommand onCopy={onCopy} />);
    expect(screen.getByText(HELPER_COMMAND)).toBeInTheDocument();
  });

  it("renders an empty command and still handles copy", () => {
    const onCopy = vi.fn();
    render(<HelperCommand command="" onCopy={onCopy} />);
    const copyButton = screen.getByRole("button", { name: "Copy" });
    expect(copyButton).toHaveAttribute("type", "button");
    fireEvent.click(copyButton);
    expect(onCopy).toHaveBeenCalledTimes(1);
  });
});