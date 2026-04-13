import { useState } from "react";
import { Box, Text, Flex } from "@radix-ui/themes";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  FileIcon,
  GearIcon,
  BarChartIcon,
  PersonIcon,
  CubeIcon,
  PlayIcon,
} from "@radix-ui/react-icons";

interface FolderItem {
  id: string;
  label: string;
  icon: React.ReactNode;
}

const menuItems: FolderItem[] = [
  { id: "setting1", label: "Setting 1", icon: <PersonIcon /> },
  { id: "setting2", label: "Setting 2", icon: <BarChartIcon /> },
  { id: "setting3", label: "Setting 3", icon: <GearIcon /> },
  { id: "setting4", label: "Setting 4", icon: <CubeIcon /> },
];

export function Sidebar() {
  const [folderOpen, setFolderOpen] = useState(true);
  const [activeItem, setActiveItem] = useState("setting1");

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
              {menuItems.map((item) => (
                <button
                  key={item.id}
                  className={`sidebar-item ${activeItem === item.id ? "active" : ""}`}
                  onClick={() => setActiveItem(item.id)}
                >
                  <Text size="2">{item.label}</Text>
                </button>
              ))}
            </Box>
          )}
        </Box>
      </Box>

      <Box style={{ padding: "0 32px 32px" }}>
        <button className="sidebar-start-btn">
          <PlayIcon width={16} height={16} />
          <Text size="3" weight="bold">
            开始运行
          </Text>
        </button>
      </Box>
    </Box>
  );
}
