import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { PULL_REQUESTS_LABEL, PullRequestLinks } from "./PullRequestLinks";

afterEach(cleanup);

describe("PullRequestLinks", () => {
  it("renders nothing without links", () => {
    const { container } = render(<PullRequestLinks links={undefined} />);
    expect(container.innerHTML).toBe("");
    render(<PullRequestLinks links={[]} />);
    expect(screen.queryByRole("list", { name: PULL_REQUESTS_LABEL })).toBeNull();
  });

  it("shows each recorded URL as text with a copy button and never as a followed link", () => {
    const { container } = render(
      <PullRequestLinks
        links={[
          { taskId: "t-1", number: 12, url: "https://github.com/octo/demo/pull/12" },
          { taskId: "t-2", number: 7, url: "https://github.com/octo/demo/pull/7" },
        ]}
      />,
    );
    const list = screen.getByRole("list", { name: PULL_REQUESTS_LABEL });
    expect(list.textContent).toContain("Pull request 12: https://github.com/octo/demo/pull/12");
    expect(list.textContent).toContain("Pull request 7: https://github.com/octo/demo/pull/7");
    expect(screen.getByRole("button", { name: "Copy the link to pull request 12" })).toBeTruthy();
    expect(container.querySelector("a")).toBeNull();
  });
});
