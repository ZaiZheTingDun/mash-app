import { useState, useCallback } from "react";
import { Box, Text, Flex } from "@radix-ui/themes";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  FileIcon,
  PlusIcon,
  PlayIcon,
  TrashIcon,
  MagnifyingGlassIcon,
} from "@radix-ui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import type { Project } from "../types/project";

interface SidebarProps {
  projects: Project[];
  onProjectsChange: (
    updater: Project[] | ((prev: Project[]) => Project[])
  ) => void;
  activeProjectId: string | null;
  onProjectSelect: (id: string) => void;
  onStartRun: () => void;
  onOpenDebug: () => void;
}

export function Sidebar({
  projects,
  onProjectsChange,
  activeProjectId,
  onProjectSelect,
  onStartRun,
  onOpenDebug,
}: SidebarProps) {
  const [folderOpen, setFolderOpen] = useState(true);

  const handleCreateProject = useCallback(() => {
    const name = `Project ${projects.length + 1}`;
    invoke<Project>("create_project", { name })
      .then((p) => {
        onProjectsChange((prev) => [...prev, p]);
        onProjectSelect(p.id);
      })
      .catch(console.error);
  }, [projects.length, onProjectSelect, onProjectsChange]);

  const handleDeleteProject = useCallback(
    (e: React.MouseEvent, id: string) => {
      e.stopPropagation();
      invoke("delete_project", { id })
        .then(() => {
          const next = projects.filter((p) => p.id !== id);
          onProjectsChange(next);
          if (activeProjectId === id && next.length > 0) {
            onProjectSelect(next[0].id);
          }
        })
        .catch(console.error);
    },
    [activeProjectId, onProjectSelect, onProjectsChange, projects]
  );

  return (
    <Box className="sidebar">
      <Box className="sidebar-content">
        <Text size="1" weight="medium" className="sidebar-label">
          MENU
        </Text>

        <Box className="sidebar-folder">
          <button
            className="sidebar-folder-toggle"
            onClick={() => setFolderOpen(!folderOpen)}
          >
            <Flex align="center" gap="3">
              {folderOpen ? (
                <ChevronDownIcon className="sidebar-chevron" />
              ) : (
                <ChevronRightIcon className="sidebar-chevron" />
              )}
              <FileIcon className="sidebar-folder-icon" />
              <Text size="2" weight="medium">
                Project
              </Text>
            </Flex>
          </button>

          {folderOpen && (
            <Box className="sidebar-items">
              {projects.map((project) => (
                <button
                  key={project.id}
                  className={`sidebar-item ${activeProjectId === project.id ? "active" : ""}`}
                  onClick={() => onProjectSelect(project.id)}
                >
                  <Flex
                    align="center"
                    justify="between"
                    style={{ width: "100%" }}
                  >
                    <Text size="2">{project.name}</Text>
                    <button
                      className="sidebar-item-delete"
                      onClick={(e) => handleDeleteProject(e, project.id)}
                    >
                      <TrashIcon width={12} height={12} />
                    </button>
                  </Flex>
                </button>
              ))}
              <button
                className="sidebar-item sidebar-add-btn"
                onClick={handleCreateProject}
              >
                <Flex align="center" gap="1">
                  <PlusIcon width={12} height={12} />
                  <Text size="2">新建项目</Text>
                </Flex>
              </button>
            </Box>
          )}
        </Box>
      </Box>

      <Box style={{ padding: "0 32px 32px" }}>
        <button className="sidebar-start-btn" onClick={onStartRun}>
          <PlayIcon width={16} height={16} />
          <Text size="3" weight="bold">
            开始运行
          </Text>
        </button>
        <button className="sidebar-debug-btn" onClick={onOpenDebug}>
          <MagnifyingGlassIcon width={14} height={14} />
          <Text size="2">CV 调试</Text>
        </button>
      </Box>
    </Box>
  );
}
