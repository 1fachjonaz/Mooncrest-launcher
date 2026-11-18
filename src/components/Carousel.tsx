import { useEffect, useState, type ReactNode } from "react";
import { useApp } from "../store";

export interface Slide {
  id: string;
  image: string;
  thumb: string;
  eyebrow: string;
  title: string;
  text?: string;
  logo?: string | null;
  actions: ReactNode;
}

const INTERVAL = 8000;

/** Store-front hero: one big slide, the list of slides on the right fills up while it plays. */
export function Carousel({ slides }: { slides: Slide[] }) {
  const { settings } = useApp();
  const [i, setI] = useState(0);
  const [paused, setPaused] = useState(false);
  const auto = !settings.reduce_motion && !window.matchMedia("(prefers-reduced-motion: reduce)").matches && slides.length > 1;

  useEffect(() => {
    if (!auto || paused) return;
    const id = setTimeout(() => setI((x) => (x + 1) % slides.length), INTERVAL);
    return () => clearTimeout(id);
  }, [i, auto, paused, slides.length]);

  if (slides.length === 0) return null;
  const s = slides[Math.min(i, slides.length - 1)];
  return (
    <section className="carousel" onMouseEnter={() => setPaused(true)} onMouseLeave={() => setPaused(false)}>
      <div className="carousel-stage">
        {slides.map((x, n) => (
          <img key={x.id} src={x.image} alt="" className={n === i ? "is-on" : ""} aria-hidden={n !== i} />
        ))}
        <div className="carousel-body" key={s.id}>
          {s.logo && <img className="carousel-logo" src={s.logo} alt="" />}
          <span className="carousel-eyebrow">{s.eyebrow}</span>
          <h2>{s.title}</h2>
          {s.text && <p>{s.text}</p>}
          <div className="carousel-actions">{s.actions}</div>
        </div>
      </div>
      {slides.length > 1 && (
        <ol className="carousel-list">
          {slides.map((x, n) => (
            <li key={x.id}>
              <button className={n === i ? "is-on" : ""} onClick={() => setI(n)} aria-current={n === i}>
                {n === i && auto && <span className={`carousel-fill ${paused ? "is-paused" : ""}`} key={`${x.id}-${i}`} style={{ animationDuration: `${INTERVAL}ms` }} />}
                <img src={x.thumb} alt="" />
                <span>{x.title}</span>
              </button>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
