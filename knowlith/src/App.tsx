import { Navigate, Route, BrowserRouter as Router, Routes } from "react-router-dom"
import { AppShell } from "@/components/AppShell"
import { TooltipProvider } from "@/components/ui/tooltip"
import { Activity } from "@/screens/Activity"
import { Brain } from "@/screens/Brain"
import { Browse } from "@/screens/Browse"
import { Connect } from "@/screens/Connect"
import { Discovery } from "@/screens/Discovery"
import { Home } from "@/screens/Home"
import { Onboarding } from "@/screens/onboarding/Onboarding"
import { Review } from "@/screens/Review"
import { Settings } from "@/screens/Settings"
import { SkillDetail } from "@/screens/SkillDetail"
import { Sources } from "@/screens/Sources"
import { Workspace } from "@/screens/Workspace"
import { AppProvider, useApp } from "@/state/AppState"

function Root() {
  const { onboarded } = useApp()
  return <Navigate to={onboarded ? "/home" : "/onboarding"} replace />
}

export function App() {
  return (
    <AppProvider>
      <TooltipProvider delayDuration={300} skipDelayDuration={200}>
        <Router>
          <Routes>
            <Route path="/" element={<Root />} />
            <Route path="/onboarding" element={<Onboarding />} />
            <Route element={<AppShell />}>
              <Route path="/home" element={<Home />} />
              <Route path="/browse" element={<Browse />} />
              <Route path="/brain" element={<Brain />} />
              <Route path="/discovery" element={<Discovery />} />
              <Route path="/workspace" element={<Navigate to="/browse" replace />} />
              <Route path="/workspace/:objectId" element={<Workspace />} />
              <Route path="/review" element={<Review />} />
              <Route path="/sources" element={<Sources />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="/connect" element={<Connect />} />
              <Route path="/activity" element={<Activity />} />
              <Route path="/skills/:skillId" element={<SkillDetail />} />
            </Route>
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </Router>
      </TooltipProvider>
    </AppProvider>
  )
}
