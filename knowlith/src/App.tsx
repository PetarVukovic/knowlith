import { Navigate, Route, BrowserRouter as Router, Routes } from "react-router-dom"
import { useEffect } from "react"
import { AppShell } from "@/components/AppShell"
import { scheduleProductTour } from "@/components/ProductTour"
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
  const { onboarded, ready, firstRun } = useApp()
  if (!ready) return null
  if (!onboarded) return <Navigate to="/onboarding" replace />
  // First landing after approve still goes review → connect. Sidebar Home
  // is not blocked the same way — see GuardHome.
  if (firstRun === "review") return <Navigate to="/review" replace />
  if (firstRun === "connect") return <Navigate to="/connect" replace />
  return <Navigate to="/home" replace />
}

function GuardHome() {
  const { firstRun, setFirstRun } = useApp()

  // Choosing Home ends the forced “connect an assistant” gate. Otherwise
  // knowlith.firstRun stays "connect" forever and every Home click bounces
  // back to AI assistants — even after the owner already uses Brain/chat.
  useEffect(() => {
    if (firstRun === "connect") {
      setFirstRun(null)
      scheduleProductTour()
    }
  }, [firstRun, setFirstRun])

  if (firstRun === "review") return <Navigate to="/review" replace />
  return <Home />
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
              <Route path="/home" element={<GuardHome />} />
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
