import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { GoalTag } from "../GoalTag";

const GB = 1e9;
afterEach(cleanup);

function goalTagReadsBelowStrictlyUnderGoal() {
  render(() => <GoalTag freeBytes={80 * GB - 1} goalBytes={80 * GB} />);
  const tag = screen.getByTestId("goal-tag");
  expect(tag).toHaveTextContent(/^Below your goal$/);
  expect(tag).toHaveClass("tag", "tag-accent");
  expect(tag).not.toHaveClass("tag-accent-2");
}

function goalTagReadsMetAtOrAboveGoal() {
  for (const free of [80 * GB, 200 * GB]) {
    render(() => <GoalTag freeBytes={free} goalBytes={80 * GB} />);
    const tag = screen.getByTestId("goal-tag");
    expect(tag).toHaveTextContent(/^Goal met$/);
    expect(tag).toHaveClass("tag", "tag-accent-2");
    expect(tag).not.toHaveClass("tag-accent");
    cleanup();
  }
}

describe("goal tag", () => {
  it("reads Below your goal with tag-accent when free < goal (REQ-DCLEAN-002.A2)", goalTagReadsBelowStrictlyUnderGoal);
  it("reads Goal met with tag-accent-2 when free >= goal (REQ-DCLEAN-002.A2)", goalTagReadsMetAtOrAboveGoal);
});
