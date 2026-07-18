import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Sidebar } from "./components/Sidebar";
import { ThreadView } from "./components/ThreadView";
import { SearchBar } from "./components/SearchBar";
import { SearchResults } from "./components/SearchResults";
import { FdaOnboarding } from "./components/FdaOnboarding";
import { GalleryView } from "./components/GalleryView";
import { LinksView } from "./components/LinksView";
import { InsightsView } from "./components/InsightsView";
import { TimelineView } from "./components/TimelineView";
import { api, isFdaError } from "./api";
import {
  useConversations,
  useFdaStatus,
  useIndexStatus,
  useIndexUpdates,
} from "./queries";
import { useContactsPermission } from "./lib/contacts";
import { formatFull } from "./lib/format";
import type { ConversationDto, SearchResultDto } from "./types";

type CenterView = "chat" | "gallery" | "links" | "insights" | "timeline";

const VIEW_TABS: { id: CenterView; label: string }[] = [
  { id: "chat", label: "Chat" },
  { id: "gallery", label: "Media" },
  { id: "links", label: "Links" },
  { id: "insights", label: "Insights" },
  { id: "timeline", label: "Timeline" },
];

export default function App() {
  const fda = useFdaStatus();

  if (fda.isLoading) {
    return <div className="splash">Starting Better iMessage…</div>;
  }

  if (!fda.data?.granted) {
    return (
      <FdaOnboarding onRecheck={() => fda.refetch()} rechecking={fda.isFetching} />
    );
  }

  return <MainLayout />;
}

function MainLayout() {
  const qc = useQueryClient();
  const conversations = useConversations(true);
  const indexStatus = useIndexStatus(true);
  const contactsPermission = useContactsPermission();

  const [selectedChat, setSelectedChat] = useState<ConversationDto | null>(null);
  const [focusMessageId, setFocusMessageId] = useState<number | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [centerView, setCenterView] = useState<CenterView>("chat");

  useIndexUpdates(selectedChat?.id ?? null);

  const convoList = useMemo(() => conversations.data ?? [], [conversations.data]);

  const selectConversation = (c: ConversationDto) => {
    setSelectedChat(c);
    setFocusMessageId(null);
    setSearchQuery("");
    setCenterView("chat");
  };

  const openResult = (r: SearchResultDto) => {
    const cid = r.chatId ?? r.canonicalChatId ?? -1;
    const existing = convoList.find((c) => c.id === cid);
    const label =
      existing?.label ?? r.chatName ?? r.chatIdentifier ?? "Conversation";
    setSelectedChat(
      existing ?? {
        id: cid,
        identifier: r.chatIdentifier ?? "",
        displayName: r.chatName,
        label,
        service: null,
        participants: [],
      },
    );
    setFocusMessageId(r.id);
    setSearchQuery("");
    setCenterView("chat");
  };

  const selectView = (v: CenterView) => {
    setCenterView(v);
    setSearchQuery("");
  };

  // A mid-session FDA revocation surfaces as an FDA-prefixed conversation error.
  if (conversations.isError && isFdaError(conversations.error)) {
    return (
      <FdaOnboarding
        rechecking={conversations.isFetching}
        onRecheck={() => {
          qc.invalidateQueries({ queryKey: ["fda"] });
          qc.invalidateQueries({ queryKey: ["conversations"] });
        }}
      />
    );
  }

  const searching = searchQuery.trim().length > 0;
  const chatScope = selectedChat?.id ?? null;

  const renderCenter = () => {
    if (searching) {
      return <SearchResults query={searchQuery} onOpenResult={openResult} />;
    }
    switch (centerView) {
      case "timeline":
        return <TimelineView />;
      case "gallery":
        return <GalleryView chatId={chatScope} />;
      case "links":
        return <LinksView chatId={chatScope} />;
      case "insights":
        return <InsightsView chatId={chatScope} />;
      default:
        return selectedChat ? (
          <ThreadView
            key={`${selectedChat.id}:${focusMessageId ?? "-"}`}
            chatId={selectedChat.id}
            title={selectedChat.label}
            focusMessageId={focusMessageId}
          />
        ) : (
          <div className="thread-empty">
            Select a conversation to start reading, or search above.
          </div>
        );
    }
  };

  return (
    <div className="app">
      <header className="topbar">
        <nav className="view-nav">
          {VIEW_TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`view-tab${
                centerView === t.id && !searching ? " active" : ""
              }`}
              onClick={() => selectView(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
        <SearchBar
          value={searchQuery}
          onChange={setSearchQuery}
          onClear={() => setSearchQuery("")}
        />
      </header>

      <div className="body">
        <Sidebar
          conversations={convoList}
          selectedId={selectedChat?.id ?? null}
          onSelect={selectConversation}
          loading={conversations.isLoading}
        />

        <main className="center">{renderCenter()}</main>
      </div>

      <footer className="statusbar">
        <span>
          {indexStatus.data
            ? `${indexStatus.data.count.toLocaleString()} messages indexed`
            : "Index status…"}
        </span>
        {contactsPermission.isBlocked ? (
          <span className="contacts-hint muted">
            Contacts access is off — showing raw numbers.{" "}
            <button
              type="button"
              className="link-button"
              onClick={() => api.openContactsSettings()}
            >
              Enable in System Settings ›
            </button>
          </span>
        ) : (
          <span className="muted">
            {indexStatus.data?.lastSynced
              ? `Last synced ${formatFull(indexStatus.data.lastSynced)}`
              : ""}
          </span>
        )}
      </footer>
    </div>
  );
}
