export interface PlaceholderProps {
  title: string;
}

/**
 * Stand-in for a screen whose own spec isn't implemented yet: the screen's
 * title and the line "Coming soon" (REQ-DCLEAN-032.A3).
 */
export function Placeholder(props: PlaceholderProps) {
  return (
    <section class="screen" data-screen-label={props.title} aria-labelledby="screen-title">
      <h1 class="screen-title" id="screen-title">
        {props.title}
      </h1>
      <p class="screen-note">Coming soon</p>
    </section>
  );
}
