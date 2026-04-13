import { useState, useEffect, useMemo } from "react";
import { Box, Flex, Text, Spinner } from "@radix-ui/themes";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar } from "./components/Sidebar";
import { StageNavigator } from "./components/StageNavigator";
import { ContentGrid, createInitialSlots } from "./components/ContentGrid";
import { CommandEditor } from "./components/CommandEditor";
import { StatusBar } from "./components/StatusBar";
import type { SlotItem } from "./components/ContentGrid";
import type { Servant } from "./types/servant";
import "./App.css";

function App() {
  const [activeStage, setActiveStage] = useState(1);
  const [servants, setServants] = useState<Servant[]>([]);
  const [slots, setSlots] = useState<SlotItem[]>(createInitialSlots);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<Servant[]>("get_servants")
      .then(setServants)
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }, []);

  const partyServants = useMemo(() => {
    const nonSupport = slots.filter((s) => s.type !== "support");
    return nonSupport.slice(0, 3).map((s) => s.servant);
  }, [slots]);

  return (
    <Flex direction="column" className="app-root">
      <Flex className="app-container">
        <Sidebar />
        <Box className="main-content">
          <Box className="main-content-inner">
            <StageNavigator
              activeStage={activeStage}
              onStageChange={setActiveStage}
            />
            {loading ? (
              <Flex align="center" justify="center" style={{ flex: 1 }}>
                <Spinner size="3" />
              </Flex>
            ) : error ? (
              <Flex
                align="center"
                justify="center"
                direction="column"
                gap="2"
                style={{ flex: 1 }}
              >
                <Text size="3" color="red" weight="medium">
                  加载从者数据失败
                </Text>
                <Text size="2" color="gray">
                  {error}
                </Text>
              </Flex>
            ) : activeStage === 3 ? (
              <CommandEditor partyServants={partyServants} />
            ) : (
              <ContentGrid
                servants={servants}
                slots={slots}
                onSlotsChange={setSlots}
              />
            )}
          </Box>
        </Box>
      </Flex>
      <StatusBar />
    </Flex>
  );
}

export default App;
