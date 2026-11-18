import { Rail, TitleBar, Toasts } from "./components/Shell";
import { Account } from "./pages/Account";
import { Discover } from "./pages/Discover";
import { Downloads } from "./pages/Downloads";
import { Home } from "./pages/Home";
import { Library } from "./pages/Library";
import { News } from "./pages/News";
import { ProductPage } from "./pages/ProductPage";
import { SettingsPage } from "./pages/Settings";
import { useApp } from "./store";

export function App() {
  const { route } = useApp();
  let page;
  if (route.product) page = <ProductPage id={route.product} />;
  else switch (route.page) {
    case "home": page = <Home />; break;
    case "library": page = <Library />; break;
    case "discover": page = <Discover />; break;
    case "downloads": page = <Downloads />; break;
    case "news": page = <News />; break;
    case "settings": page = <SettingsPage />; break;
    case "account": page = <Account />; break;
  }
  return (
    <div className="shell">
      <Rail />
      <div className="main">
        <TitleBar />
        <main className="content" key={`${route.page}:${route.product ?? ""}:${route.q ?? ""}`}>{page}</main>
      </div>
      <Toasts />
    </div>
  );
}
