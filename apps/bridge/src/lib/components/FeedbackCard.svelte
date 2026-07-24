<script lang="ts">
  import type { FeedbackState } from '$lib/types';

  let { feedback }: { feedback: FeedbackState | null } = $props();
</script>

<section class="card">
  <h2>Desktop Feedback</h2>
  {#if feedback}
    <div class="feedback-grid">
      <div class="feedback-cell">
        <span class="feedback-label">Status</span>
        <span class="feedback-value">
          {#if feedback.presenting}
            <span class="chip chip-green">Presenting</span>
          {:else}
            <span class="chip">Idle</span>
          {/if}
        </span>
      </div>
      <div class="feedback-cell">
        <span class="feedback-label">Slide</span>
        <span class="feedback-value">
          {#if feedback.currentSlide != null && feedback.slideCount != null}
            {feedback.currentSlide} / {feedback.slideCount}
          {:else}
            <span class="muted">—</span>
          {/if}
        </span>
      </div>
      <div class="feedback-cell wide">
        <span class="feedback-label">Presentation</span>
        <span class="feedback-value">{feedback.presentationName ?? '—'}</span>
      </div>
      <div class="feedback-cell wide">
        <span class="feedback-label">Last command</span>
        <span class="feedback-value muted">
          {feedback.lastCommand ? feedback.lastCommand : '—'}
          {feedback.lastCommandTime ? `at ${new Date(feedback.lastCommandTime).toLocaleTimeString()}` : ''}
        </span>
      </div>
    </div>
    {#if feedback.slideNotes}
      <div class="notes">
        <strong>Notes</strong>
        <p>{feedback.slideNotes}</p>
      </div>
    {/if}
  {:else}
    <p class="empty">No feedback yet. Desktops will send slide status here once they connect.</p>
  {/if}
</section>
