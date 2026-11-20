/** Settings (`#/settings?page=<id>`): seven pages behind a left menu, one shown at a time. Rendered inside the shell's `main` landmark, so it adds none. */
import { useId, useState, type KeyboardEvent } from "react";
import { useServices, useStore } from "../../services";
import { Badge } from "../../theme";
import { SettingsPage } from "./pageSave";
import { PAGES, searchSettings } from "./settingsPages";
import { useSettings } from "./useSettings";
import "./SettingsView.css";

export function SettingsView() {
  const model = useSettings();
  const { router } = useServices();
  const route = useStore(router);
  const [query, setQuery] = useState("");
  const searchId = useId();
  const requested = route.name === "global" ? route.settingsPage : undefined;
  // A bare `#/settings`, an old link, or an unknown page opens the first page.
  const page = PAGES.find((candidate) => candidate.id === requested) ?? PAGES[0]!;
  const results = searchSettings(query);

  const open = (id: string) => {
    setQuery("");
    router.navigate({ name: "global", view: "settings", settingsPage: id });
  };
  const onSearchKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" && results[0]) open(results[0].page);
    if (event.key === "Escape") setQuery("");
  };

  return (
    <div className="settings-view" data-testid="settings-view">
      <header className="settings-head">
        <h1>Settings</h1>
        {!model.live && <Badge tone="placeholder">Fixture data</Badge>}
      </header>
      <div className="settings-layout">
        <div className="settings-menu">
          <input
            id={searchId}
            type="search"
            className="settings-search"
            aria-label="Find a setting"
            placeholder="Find a setting"
            autoComplete="off"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={onSearchKey}
          />
          {query.trim() === "" ? (
            <nav aria-label="Settings pages">
              <ul className="settings-menu-list">
                {PAGES.map((item) => (
                  <li key={item.id}>
                    <button
                      type="button"
                      className="settings-menu-item"
                      aria-current={item.id === page.id ? "page" : undefined}
                      onClick={() => open(item.id)}
                    >
                      {item.label}
                    </button>
                  </li>
                ))}
              </ul>
            </nav>
          ) : (
            <ul className="settings-menu-list" aria-label="Search results">
              {results.length === 0 && <li className="settings-hint">No setting matches.</li>}
              {results.map((entry) => (
                <li key={`${entry.page}:${entry.label}`}>
                  <button type="button" className="settings-menu-item" onClick={() => open(entry.page)}>
                    {entry.label}
                    <small className="settings-hint">{PAGES.find((item) => item.id === entry.page)?.label}</small>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="settings-page" data-testid={`settings-page-${page.id}`}>
          <h2>{page.label}</h2>
          <p className="settings-intro">{page.intro}</p>
          <SettingsPage key={page.id}>
            {page.sections.map((Section, index) => (
              <Section key={index} model={model} />
            ))}
          </SettingsPage>
        </div>
      </div>
    </div>
  );
}
