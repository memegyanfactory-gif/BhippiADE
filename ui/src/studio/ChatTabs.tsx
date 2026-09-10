import { ProviderLogo } from "../components/ProviderLogo";
import { IconClose, IconPlus } from "../components/icons";
import { chatTabTitle, type ChatTabSession } from "./chatTabs.ts";

/**
 * The studio's chat tab strip.
 *
 * The owner's ask, verbatim: "on the top it should look like tabs so user can click and add
 * more chat/tabs if needed and can talk to any provider". So the top of the chat column is a
 * browser-style strip: one tab per chat in this project, the active one lit, a `+` that opens
 * another, and a ✕ that closes one. Each conversation already remembers its own provider and
 * model, so the glyph on a tab is not decoration — it is which model that chat is talking to.
 *
 * This replaces the old `.chat-top-bar`, whose row of identical gear buttons (one per
 * activated plugin) was the "many settings icons" complaint. Plugin launchers belong to the
 * Add-ons screen; the studio strip carries chats and nothing else.
 *
 * ✕ deletes the conversation. That is Bhippi's existing meaning for closing a chat — the same
 * one the rail and the old top bar had — so it is spelled out in the button's title rather
 * than softened into something the strip does not actually do.
 */

interface ChatTabsProps {
  /** The chats of the active project, already selected and ordered by `chatTabsFor`. */
  tabs: readonly (ChatTabSession & { provider: string | null; provider_label: string | null })[];
  activeId: string | null;
  onOpen: (id: string) => void;
  /** Closing a chat deletes it — App wires this to `deleteConversation`. */
  onClose: (id: string) => void;
  onNew: () => void;
}

export function ChatTabs({ tabs, activeId, onOpen, onClose, onNew }: ChatTabsProps) {
  return (
    // Plain buttons, deliberately: `role="tablist"` would promise arrow-key roving focus and
    // a `tabpanel` this strip does not have. Every tab is reachable with Tab, activated with
    // Enter or Space, and the current one says so with `aria-current`.
    <div className="studio-chat-tabs" role="group" aria-label="Chats in this project">
      <div className="studio-chat-tabs-scroll">
        {tabs.map((tab) => {
          const label = chatTabTitle(tab.title);
          const active = tab.id === activeId;
          return (
            <div key={tab.id} className={`studio-chat-tab${active ? " active" : ""}`}>
              <button
                type="button"
                aria-current={active ? true : undefined}
                className="studio-chat-tab-open"
                onClick={() => onOpen(tab.id)}
                title={tab.provider_label ? `${label} — ${tab.provider_label}` : label}
              >
                {tab.provider ? (
                  <ProviderLogo id={tab.provider} size={13} />
                ) : (
                  <span className="studio-chat-tab-dot" aria-hidden="true" />
                )}
                <span className="studio-chat-tab-title">{label}</span>
              </button>
              <button
                type="button"
                className="studio-chat-tab-close"
                onClick={() => onClose(tab.id)}
                title="Close this chat (it is deleted)"
                aria-label={`Close ${label}`}
              >
                <IconClose size={10} />
              </button>
            </div>
          );
        })}
      </div>
      {/* Outside the scroller: with twenty chats open, "new chat" must still be one click. */}
      <button
        type="button"
        className="studio-chat-tab-new"
        onClick={onNew}
        title="Start a new chat"
        aria-label="Start a new chat"
      >
        <IconPlus size={13} />
      </button>
    </div>
  );
}
