import { useState } from "react";
import { Box, Flex } from "@radix-ui/themes";
import { Sidebar } from "./components/Sidebar";
import { StageNavigator } from "./components/StageNavigator";
import { ContentGrid } from "./components/ContentGrid";
import "./App.css";

function App() {
  const [activeStage, setActiveStage] = useState(1);

  return (
    <Flex className="app-container">
      <Sidebar />
      <Box className="main-content">
        <Box className="main-content-inner">
          <StageNavigator
            activeStage={activeStage}
            onStageChange={setActiveStage}
          />
          <ContentGrid />
        </Box>
      </Box>
    </Flex>
  );
}

export default App;
