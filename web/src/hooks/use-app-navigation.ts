import { useCallback, useEffect, useMemo, useState } from 'react';

import type { SidebarSection } from '../components/app/types';

interface NavigableItem {
  id: string;
}

interface UseAppNavigationOptions<Board extends NavigableItem, Project extends NavigableItem> {
  boards: Board[];
  projects: Project[];
  startupPage: SidebarSection;
}

export function useAppNavigation<Board extends NavigableItem, Project extends NavigableItem>({
  boards,
  projects,
  startupPage,
}: UseAppNavigationOptions<Board, Project>) {
  const [activeBoardId, setActiveBoardId] = useState<string | null>(null);
  const [sidebarSection, setSidebarSection] = useState<SidebarSection>(startupPage);

  const activeBoard = useMemo(
    () => boards.find((board) => board.id === activeBoardId) ?? null,
    [activeBoardId, boards],
  );
  const activeProject = useMemo(
    () => projects.find((project) => project.id === activeBoardId) ?? null,
    [activeBoardId, projects],
  );

  const openBoard = useCallback((boardId: string) => {
    setActiveBoardId(boardId);
    setSidebarSection('boards');
  }, []);

  const clearActiveBoard = useCallback(() => {
    setActiveBoardId(null);
    setSidebarSection('boards');
  }, []);

  useEffect(() => {
    if (!activeBoardId) {
      return;
    }

    if (projects.some((project) => project.id === activeBoardId)) {
      return;
    }

    clearActiveBoard();
  }, [activeBoardId, clearActiveBoard, projects]);

  return {
    activeBoard,
    activeBoardId,
    activeProject,
    clearActiveBoard,
    openBoard,
    setSidebarSection,
    sidebarSection,
  };
}
