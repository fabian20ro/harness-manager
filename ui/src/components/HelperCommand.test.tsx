import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { HelperCommand } from "./HelperCommand";

describe("HelperCommand", () => {
  it("renders cargo run and copy button", () => {
    const onCopy = vi.fn();
    render(<HelperCommand onCopy={onCopy} />);

    expect(screen.getByText("cargo run")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(onCopy).toHaveBeenCalledTimes(1);
  });
});
