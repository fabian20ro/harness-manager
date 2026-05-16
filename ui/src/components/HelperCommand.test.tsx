import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { HelperCommand } from "./HelperCommand";

describe("HelperCommand", () => {
  it("renders cargo run and copy button", () => {
    const onCopy = vi.fn();
    render(<HelperCommand onCopy={onCopy} />);

    expect(screen.getByText("cargo run")).toBeInTheDocument();
    const copyButton = screen.getByRole("button", { name: "Copy" });
    expect(copyButton).toHaveAttribute("type", "button");
    fireEvent.click(copyButton);
    expect(onCopy).toHaveBeenCalledTimes(1);
  });
});
