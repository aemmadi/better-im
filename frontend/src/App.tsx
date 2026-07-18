import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Sidebar } from "./components/Sidebar";
import { ThreadView } from "./components/ThreadView";
import { SearchBar } from "./components/SearchBar";
import { SearchResults } from "./components/SearchResults";
import { FdaOnboarding } from "./components/FdaOnboarding";
import { isFdaError } from "./api";
import {
  useConversations,
  useFdaStatus,
  useIndexStatus,
  useIndexUpdates,
} from "./queries";
import { formatFull } from "./lib/format";
import type { ConversationDto, SearchResultDto } from "./types";

export default function App() {
  const fda = useFdaStatus();

  if (fda.isLoading) {
    return <div className="splash">Starting Better iMessage…</div>;
  }

  if (!fda.data?.granted) {
    return (
      <FdaOnboarding
        onRecheck={() => fda.refetch()}
        rechecking={fda.isFetching}
      />
    );
  }

  return <MainLayout />;
}

function MainLayout() {
  const qc = useQueryClient();
  const conversations = useConversations(true);
  const indexStatus = useIndexStatus(true);

  const [selectedChat, setSelectedChat] = useState<ConversationDto | null>(null);
  const [focusMessageId, setFocusMessageId] = useState<number | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  useIndexUpdates(selectedChat?.id ?? null);

  const convoList = useMemo(() => conversations.data ?? [], [conversations.data]);

  const selectConversation = (c: ConversationDto) => {
    setSelectedChat(c);
    setFocusMessageId(null);
    setSearchQuery("");
  };

  const openResult = (r: SearchResultDto) => {
    const cid = r.chatId ?? r.canonicalChatId ?? -1;
    const existing = convoList.find((c) => c.id === cid);
    const label = existing?.label ?? r.chatName ?? r.chatIdentifier ?? "Conversation";
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

  return (
    <div className="app">
      <header className="topbar">
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

        <main className="center">
          {searching ? (
            <SearchResults query={searchQuery} onOpenResult={openResult} />
          ) : selectedChat ? (
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
          )}
        </main>
      </div>

      <footer className="statusbar">
        <span>
          {indexStatus.data
            ? `${indexStatus.data.count.toLocaleString()} messages indexed`
            : "Index status…"}
        </span>
        <span className="muted">
          {indexStatus.data?.lastSynced
            ? `Last synced ${formatFull(indexStatus.data.lastSynced)}`
            : ""}
        </span>
      </footer>
    </div>
  );
}
