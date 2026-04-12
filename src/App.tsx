import { useState, useEffect } from "react";
import { Box, Flex, Text, Spinner } from "@radix-ui/themes";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar } from "./components/Sidebar";
import { StageNavigator } from "./components/StageNavigator";
import { ContentGrid } from "./components/ContentGrid";
import type { Servant } from "./types/servant";
import "./App.css";

function App() {
  const [activeStage, setActiveStage] = useState(1);
  const [servants, setServants] = useState<Servant[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<Servant[]>("get_servants")
      .then(setServants)
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }, []);

  return (
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
          ) : (
            <ContentGrid servants={servants} />
          )}
        </Box>
      </Box>
    </Flex>
  );
}

export default App;
