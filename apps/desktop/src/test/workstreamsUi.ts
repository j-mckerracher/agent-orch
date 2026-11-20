/** Click sequences for the two-screen Workstreams view (list screen, then one workstream's detail screen). */
import { screen, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";

export type User = UserEvent;

/** List screen: opens the workstream whose row shows `title`. */
export async function openWorkstream(user: User, title: string): Promise<void> {
  const rows = await screen.findAllByTestId("ws-row");
  const row = rows.find((candidate) => within(candidate).queryByText(title) !== null);
  if (!row) throw new Error(`No workstream row titled "${title}".`);
  await user.click(row);
  await screen.findByRole("heading", { level: 1, name: title });
}

/** Detail screen: the "← Workstreams" button. */
export async function backToList(user: User): Promise<void> {
  await user.click(within(screen.getByTestId("ws-detail")).getByRole("button", { name: "Workstreams" }));
  await screen.findByRole("heading", { level: 1, name: "Workstreams" });
}

/** Selects a tab by its name without its count: "Tasks" selects "Tasks 2". */
export async function openTab(user: User, name: string): Promise<void> {
  await user.click(await screen.findByRole("tab", { name: new RegExp(`^${name}( \\d+)?$`) }));
}

/** Opens a menu button ("Workstream actions" or "More actions") and returns its menu. */
export async function openMenu(user: User, menu: string): Promise<HTMLElement> {
  await user.click(await screen.findByRole("button", { name: menu }));
  return screen.getByRole("menu", { name: menu });
}

/** Opens a menu and chooses one item. A disabled item does nothing, as for a user. */
export async function chooseAction(user: User, menu: string, item: string | RegExp): Promise<void> {
  const opened = await openMenu(user, menu);
  await user.click(within(opened).getByRole("menuitem", { name: item }));
}
