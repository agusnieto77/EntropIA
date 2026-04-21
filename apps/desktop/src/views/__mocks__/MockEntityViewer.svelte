<script lang="ts">
  let { entities = [], onentityclick = () => {}, onhighlight = () => {} } = $props()
</script>

<div data-testid="mock-entity-viewer">
  {#each entities as entity (entity.id)}
    <button
      type="button"
      data-testid={`mock-entity-${entity.id}`}
      onclick={() => {
        if (entity.startOffset != null && entity.endOffset != null) {
          onhighlight({ startOffset: entity.startOffset, endOffset: entity.endOffset })
        }
        onentityclick(entity)
      }}
    >
      {entity.value}
    </button>
  {/each}
</div>
