<script lang="ts">
  import { t } from '$lib/i18n'
  import { navigation } from '$lib/navigation'
  import {
    pickCsvFile,
    parseCsvFile,
    importCsv,
    suggestMapping,
    type CsvParseResult,
    type CsvColumnMapping,
    type CsvImportProgress,
  } from '$lib/csv-import'

  let { onClose }: { onClose: () => void } = $props()

  // State machine: 'pick' → 'configure' → 'importing' → 'done'
  let step = $state<'pick' | 'configure' | 'importing' | 'done'>('pick')
  let error = $state<string | null>(null)

  // Parse result
  let parsed = $state<CsvParseResult | null>(null)

  // Form fields
  let collectionName = $state('')
  let collectionDescription = $state('')
  let itemName = $state('')
  let sourceLabel = $state('')

  // Column mapping
  let textColumn = $state('')
  let titleColumn = $state<string | null>(null)
  let dateColumn = $state<string | null>(null)
  let idColumn = $state<string | null>(null)

  // Import progress
  let progress = $state<CsvImportProgress>({ current: 0, total: 0, phase: 'creating' })
  let resultCollectionId = $state<string | null>(null)
  let resultItemId = $state<string | null>(null)
  let resultEntryCount = $state(0)
  let resultSkipped = $state(0)

  let canImport = $derived(!!parsed && !!collectionName.trim() && !!itemName.trim() && !!textColumn)

  async function handlePickFile() {
    error = null
    try {
      const filePath = await pickCsvFile()
      if (!filePath) return

      parsed = await parseCsvFile(filePath)
      const baseName = parsed.fileName.replace(/\.[^.]+$/, '')
      collectionName = baseName
      itemName = baseName

      // Auto-suggest mapping
      const suggested = suggestMapping(parsed.columns)
      textColumn = suggested.textColumn
      titleColumn = suggested.titleColumn
      dateColumn = suggested.dateColumn
      idColumn = suggested.idColumn

      step = 'configure'
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    }
  }

  async function handleImport() {
    if (!parsed || !canImport) return
    error = null
    step = 'importing'

    const mapping: CsvColumnMapping = {
      textColumn,
      titleColumn,
      dateColumn,
      idColumn,
    }

    try {
      const result = await importCsv(
        parsed,
        {
          collectionName: collectionName.trim(),
          collectionDescription: collectionDescription.trim() || null,
          itemName: itemName.trim(),
          mapping,
          sourceLabel: sourceLabel.trim() || null,
        },
        (p) => {
          progress = p
        }
      )
      resultCollectionId = result.collectionId
      resultItemId = result.itemId
      resultEntryCount = result.entryCount
      resultSkipped = result.skippedDuplicates
      step = 'done'
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
      step = 'configure'
    }
  }

  function handleGoToCollection() {
    if (!resultCollectionId) return
    navigation.navigate({
      name: 'collection',
      id: resultCollectionId,
      collectionName: collectionName.trim(),
    })
    onClose()
  }

  function handleImportAnother() {
    parsed = null
    collectionName = ''
    collectionDescription = ''
    itemName = ''
    sourceLabel = ''
    textColumn = ''
    titleColumn = null
    dateColumn = null
    idColumn = null
    resultCollectionId = null
    resultItemId = null
    resultEntryCount = 0
    resultSkipped = 0
    error = null
    step = 'pick'
  }
</script>

<div class="csv-importer">
  <div class="csv-importer__header">
    <h2>{t('csvImport.title')}</h2>
    <p class="csv-importer__subtitle">{t('csvImport.subtitle')}</p>
  </div>

  {#if error}
    <div class="csv-importer__error">{error}</div>
  {/if}

  <!-- Step 1: Pick file -->
  {#if step === 'pick'}
    <div class="csv-importer__center">
      <button class="csv-importer__pick-btn" onclick={handlePickFile}>
        {t('csvImport.selectFile')}
      </button>
    </div>

  <!-- Step 2: Configure mapping -->
  {:else if step === 'configure' && parsed}
    <div class="csv-importer__file-info">
      {t('csvImport.fileSelected', { fileName: parsed.fileName, rowCount: parsed.totalRows })}
      <button class="csv-importer__link-btn" onclick={handlePickFile}>
        {t('csvImport.changeFile')}
      </button>
    </div>

    <div class="csv-importer__form">
      <!-- Collection info -->
      <div class="csv-importer__field">
        <label for="csv-collection-name">{t('csvImport.collectionName')}</label>
        <input
          id="csv-collection-name"
          type="text"
          bind:value={collectionName}
          placeholder={t('csvImport.collectionNamePlaceholder')}
        />
      </div>
      <div class="csv-importer__field">
        <label for="csv-collection-desc">{t('csvImport.collectionDescription')}</label>
        <input
          id="csv-collection-desc"
          type="text"
          bind:value={collectionDescription}
          placeholder={t('csvImport.collectionDescriptionPlaceholder')}
        />
      </div>
      <div class="csv-importer__field">
        <label for="csv-item-name">{t('csvImport.itemName')}</label>
        <input
          id="csv-item-name"
          type="text"
          bind:value={itemName}
          placeholder={t('csvImport.itemNamePlaceholder')}
        />
      </div>
      <div class="csv-importer__field">
        <label for="csv-source-label">{t('csvImport.sourceLabel')}</label>
        <input
          id="csv-source-label"
          type="text"
          bind:value={sourceLabel}
          placeholder={t('csvImport.sourceLabelPlaceholder')}
        />
      </div>

      <!-- Column mapping -->
      <div class="csv-importer__mapping">
        <h3>{t('csvImport.mappingTitle')}</h3>
        <p class="csv-importer__mapping-subtitle">{t('csvImport.mappingSubtitle')}</p>

        <div class="csv-importer__mapping-grid">
          <div class="csv-importer__mapping-row">
            <label for="csv-map-text">{t('csvImport.textColumn')} *</label>
            <select id="csv-map-text" bind:value={textColumn}>
              {#each parsed.columns as col}
                <option value={col.name}>{col.name}</option>
              {/each}
            </select>
            <span class="csv-importer__help">{t('csvImport.textColumnHelp')}</span>
          </div>

          <div class="csv-importer__mapping-row">
            <label for="csv-map-title">{t('csvImport.titleColumn')}</label>
            <select id="csv-map-title" bind:value={titleColumn}>
              <option value={null}>{t('csvImport.none')}</option>
              {#each parsed.columns as col}
                <option value={col.name}>{col.name}</option>
              {/each}
            </select>
            <span class="csv-importer__help">{t('csvImport.titleColumnHelp')}</span>
          </div>

          <div class="csv-importer__mapping-row">
            <label for="csv-map-date">{t('csvImport.dateColumn')}</label>
            <select id="csv-map-date" bind:value={dateColumn}>
              <option value={null}>{t('csvImport.none')}</option>
              {#each parsed.columns as col}
                <option value={col.name}>{col.name}</option>
              {/each}
            </select>
            <span class="csv-importer__help">{t('csvImport.dateColumnHelp')}</span>
          </div>

          <div class="csv-importer__mapping-row">
            <label for="csv-map-id">{t('csvImport.idColumn')}</label>
            <select id="csv-map-id" bind:value={idColumn}>
              <option value={null}>{t('csvImport.none')}</option>
              {#each parsed.columns as col}
                <option value={col.name}>{col.name}</option>
              {/each}
            </select>
            <span class="csv-importer__help">{t('csvImport.idColumnHelp')}</span>
          </div>
        </div>
      </div>

      <!-- Preview table -->
      <div class="csv-importer__preview">
        <h3>{t('csvImport.preview')}</h3>
        <div class="csv-importer__table-wrap">
          <table>
            <thead>
              <tr>
                {#each parsed.columns as col}
                  <th class:mapped={col.name === textColumn || col.name === titleColumn || col.name === dateColumn || col.name === idColumn}>
                    {col.name}
                  </th>
                {/each}
              </tr>
            </thead>
            <tbody>
              {#each parsed.rows.slice(0, 5) as row}
                <tr>
                  {#each parsed.columns as col}
                    <td class:mapped={col.name === textColumn} title={row[col.name] ?? ''}>
                      {(row[col.name] ?? '').slice(0, 120)}
                    </td>
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>

      <!-- Actions -->
      <div class="csv-importer__actions">
        <button class="csv-importer__btn csv-importer__btn--secondary" onclick={onClose}>
          {t('csvImport.back')}
        </button>
        <button
          class="csv-importer__btn csv-importer__btn--primary"
          onclick={handleImport}
          disabled={!canImport}
        >
          {t('csvImport.import')}
        </button>
      </div>
    </div>

  <!-- Step 3: Importing -->
  {:else if step === 'importing'}
    <div class="csv-importer__progress">
      <p>{t('csvImport.importing', { current: progress.current, total: progress.total })}</p>
      <div class="csv-importer__progress-bar">
        <div
          class="csv-importer__progress-fill"
          style="width: {progress.total > 0 ? (progress.current / progress.total) * 100 : 0}%"
        ></div>
      </div>
    </div>

  <!-- Step 4: Done -->
  {:else if step === 'done'}
    <div class="csv-importer__done">
      <p class="csv-importer__success">
        {t('csvImport.done', { count: resultEntryCount })}
        {#if resultSkipped > 0}
          <br /><span class="csv-importer__skipped">{t('csvImport.skipped', { count: resultSkipped })}</span>
        {/if}
      </p>
      <div class="csv-importer__actions">
        <button class="csv-importer__btn csv-importer__btn--secondary" onclick={handleImportAnother}>
          {t('csvImport.importAnother')}
        </button>
        <button class="csv-importer__btn csv-importer__btn--primary" onclick={handleGoToCollection}>
          {t('csvImport.goToCollection')}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .csv-importer {
    max-width: 800px;
    margin: 0 auto;
    padding: var(--space-6);
  }
  .csv-importer__header {
    margin-bottom: var(--space-6);
  }
  .csv-importer__header h2 {
    font-size: var(--font-size-lg);
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 0 0 var(--space-1) 0;
  }
  .csv-importer__subtitle {
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    margin: 0;
  }
  .csv-importer__error {
    background: color-mix(in srgb, var(--color-danger) 10%, transparent);
    color: var(--color-danger);
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-md);
    margin-bottom: var(--space-4);
    font-size: var(--font-size-sm);
  }
  .csv-importer__center {
    display: flex;
    justify-content: center;
    padding: var(--space-8) 0;
  }
  .csv-importer__pick-btn {
    padding: var(--space-3) var(--space-6);
    border-radius: var(--radius-md);
    border: 1px dashed var(--color-border-subtle);
    background: var(--color-surface);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
  }
  .csv-importer__pick-btn:hover {
    border-color: var(--color-accent);
    background: var(--color-surface-raised);
  }
  .csv-importer__file-info {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    margin-bottom: var(--space-4);
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .csv-importer__link-btn {
    background: none;
    border: none;
    color: var(--color-accent);
    cursor: pointer;
    font-size: var(--font-size-sm);
    text-decoration: underline;
    padding: 0;
  }
  .csv-importer__form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .csv-importer__field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .csv-importer__field label {
    font-size: var(--font-size-sm);
    font-weight: 500;
    color: var(--color-text-primary);
  }
  .csv-importer__field input {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-hairline);
    background: var(--color-surface-sunken);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    min-height: var(--control-height-md);
  }
  .csv-importer__field input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: var(--focus-ring);
    background: var(--color-surface);
  }
  .csv-importer__mapping {
    border-top: 1px solid var(--color-border-subtle);
    padding-top: var(--space-4);
  }
  .csv-importer__mapping h3 {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 0 0 var(--space-1) 0;
  }
  .csv-importer__mapping-subtitle {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    margin: 0 0 var(--space-3) 0;
  }
  .csv-importer__mapping-grid {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .csv-importer__mapping-row {
    display: grid;
    grid-template-columns: 140px 1fr;
    grid-template-rows: auto auto;
    gap: var(--space-1) var(--space-3);
    align-items: center;
  }
  .csv-importer__mapping-row label {
    font-size: var(--font-size-sm);
    font-weight: 500;
    color: var(--color-text-primary);
  }
  .csv-importer__mapping-row select {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-hairline);
    background: var(--color-surface-sunken);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    min-height: var(--control-height-md);
  }
  .csv-importer__mapping-row select:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: var(--focus-ring);
  }
  .csv-importer__help {
    grid-column: 2;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }
  .csv-importer__preview {
    border-top: 1px solid var(--color-border-subtle);
    padding-top: var(--space-4);
  }
  .csv-importer__preview h3 {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 0 0 var(--space-3) 0;
  }
  .csv-importer__table-wrap {
    overflow-x: auto;
    border: 1px solid var(--color-hairline);
    border-radius: var(--radius-md);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-size-xs);
  }
  th, td {
    padding: var(--space-2) var(--space-3);
    text-align: left;
    border-bottom: 1px solid var(--color-hairline);
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  th {
    background: var(--color-surface-raised);
    font-weight: 600;
    color: var(--color-text-secondary);
    position: sticky;
    top: 0;
  }
  th.mapped {
    color: var(--color-accent);
  }
  td.mapped {
    background: color-mix(in srgb, var(--color-accent) 5%, transparent);
  }
  .csv-importer__actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-3);
    padding-top: var(--space-4);
  }
  .csv-importer__btn {
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
    border: 1px solid var(--color-hairline);
    transition:
      background-color var(--transition-base),
      border-color var(--transition-base);
  }
  .csv-importer__btn--primary {
    background: var(--color-accent);
    color: #fff;
    border-color: var(--color-accent);
  }
  .csv-importer__btn--primary:hover:not(:disabled) {
    background: var(--color-accent-hover);
    border-color: var(--color-accent-hover);
  }
  .csv-importer__btn--primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .csv-importer__btn--secondary {
    background: var(--color-surface-raised);
    color: var(--color-text-primary);
  }
  .csv-importer__btn--secondary:hover {
    background: var(--color-surface-elevated);
    border-color: var(--color-border-strong);
  }
  .csv-importer__progress {
    padding: var(--space-8) 0;
    text-align: center;
  }
  .csv-importer__progress p {
    color: var(--color-text-secondary);
    margin-bottom: var(--space-4);
  }
  .csv-importer__progress-bar {
    height: 6px;
    background: var(--color-surface-sunken);
    border-radius: 3px;
    overflow: hidden;
  }
  .csv-importer__progress-fill {
    height: 100%;
    background: var(--color-accent);
    transition: width 0.2s ease;
    border-radius: 3px;
  }
  .csv-importer__done {
    padding: var(--space-8) 0;
    text-align: center;
  }
  .csv-importer__success {
    color: var(--color-success);
    font-weight: var(--font-weight-medium);
    margin-bottom: var(--space-6);
  }
  .csv-importer__skipped {
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    font-weight: 400;
  }
  .csv-importer__done .csv-importer__actions {
    justify-content: center;
  }
</style>
