import { Routes, Route, Navigate } from "react-router-dom";
import { Toaster } from "sonner";

import NotesPage from "@/pages/NotesPage";
import NoteDetailPage from "@/pages/NoteDetailPage";
import SettingsPage from "@/pages/SettingsPage";
import SetupGate from "@/components/SetupGate";

export default function App() {
  return (
    <>
      <SetupGate>
        <Routes>
          <Route path="/" element={<Navigate to="/notes" replace />} />
          <Route path="/notes" element={<NotesPage />} />
          <Route path="/notes/:id" element={<NoteDetailPage />} />
          <Route path="/settings" element={<SettingsPage />} />
        </Routes>
      </SetupGate>
      <Toaster position="bottom-right" richColors />
    </>
  );
}
