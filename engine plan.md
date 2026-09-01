# BHIPPI CREATOR — PRODUCTION-GRADE 3D GAME ENGINE MASTER BUILD PROMPT

You are working inside an existing application called **Bhippi Creator**.

Bhippi Creator is an AI-native creation environment where users can select different AI providers/models, chat with them, write code, create applications, create games, edit files, and work inside an integrated 3D environment.

The current 3D section must now be transformed into a **real, production-grade game engine and real-time 3D creation environment**.

This is NOT a UI mockup.

This is NOT a fake editor.

This is NOT a visual prototype.

This is NOT a collection of buttons that do nothing.

This is NOT a simplified Three.js scene viewer pretending to be a game engine.

Every major system described below must have a proper architecture, real state, real data structures, real engine APIs, real runtime behavior, proper editor integration, serialization, error handling, performance considerations, and AI-accessible interfaces.

The goal is to create an engine comparable in workflow and capabilities to modern professional engines such as Unreal Engine, Unity, Godot, Blender's 3D environment, and similar professional creation tools while building Bhippi's implementation and architecture independently.

Do not copy proprietary Unreal Engine source code, assets, shaders, icons, UI, names, or internal implementation.

Build Bhippi Engine as its own system.

---

# STATUS TRACKER (check this first)

Real working depth over fake breadth. Check a row only when code + tests exist. Write the files created.

| § | System | State | Evidence |
|---|---|---|---|
| 4 | ECS / scene document | in progress | `crates/bhippi-engine/src/document.rs` entities+components; UI still a JSON document. `parse_lenient` upgrades `ent_*` ids. |
| 5 | Scene graph | in progress | parent/child in `.bscn.json`; Hierarchy panel. |
| 40 | Content Browser | in progress | `EngineContentDrawer.tsx` lists real project files. |
| 48 | Editor viewport | in progress | Three.js viewport, lit/wireframe, fly camera. Bevy child process still remaining. |
| 49 | Transform gizmos | **done** | `EngineViewport.tsx` TransformControls W/E/R + RGB axis widget top-right. World/local (X), snap 10/1/0.1, Dup/Del. |
| 50 | Selection | in progress | click select + yellow box; Delete/Ctrl+D; no marquee yet. |
| 51 | Inspector | in progress | transform + material maps. |
| 57 | Play mode | in progress | Play composes Main+HUD+level; WASD. |
| 73 | Editor layout | in progress | Outliner / Viewport / Inspector / Content Drawer. |
| 76 | AI ↔ Engine bridge | **done this slice** | `engine_query_scene` + `engine_apply_action` IPC; `<engine_action>` parsed in `chat.rs`; `prompts/chat-engine.md` injected when a game exists. |
| 77 | Engine schema for AI | in progress | `schema.rs` registry + mindmap digest. |
| 1–3, 6–39, 41–47, 52–56, 58–75, 78–150 | Everything else | todo | Not started as real subsystems. Do not fake UI for them. |

**This slice (2026-08-30):** chat providers no longer starved by a 10s full detect; Engine snap / world-local / duplicate / delete / undo.

---

# 1. CORE PRODUCT PHILOSOPHY

Bhippi Creator should combine:

* AI coding environment
* professional game engine
* real-time 3D editor
* scene editor
* rendering engine
* material editor
* shader system
* animation system
* physics engine
* scripting environment
* visual scripting
* particle/VFX tools
* terrain tools
* lighting tools
* audio engine
* UI/game HUD editor
* asset manager
* profiler
* debugging tools
* build/export system
* plugin architecture
* AI agent control

The engine should allow a user to create an entire game from inside Bhippi Creator without needing another editor.

A user should be able to say:

"Create a third-person shooter."

And the AI should be capable of:

1. understanding the current project
2. creating folders and assets
3. creating scenes
4. creating entities
5. importing/generated assets
6. creating materials
7. adding lights
8. writing scripts
9. setting up physics
10. creating player controls
11. creating animation state machines
12. building UI
13. configuring cameras
14. creating game logic
15. running the game
16. detecting errors
17. fixing errors
18. visually inspecting the scene
19. editing entities through engine APIs
20. rerunning the project
21. testing behavior
22. profiling performance
23. building/exporting the final application

The AI should use the same underlying engine commands that human users use.

---

# 2. ENGINE ARCHITECTURE

Create a strongly modular engine architecture.

Recommended top-level architecture:

```text
/apps
    /creator
    /player
    /launcher

/engine
    /core
    /runtime
    /render
    /graphics
    /scene
    /ecs
    /physics
    /animation
    /audio
    /navigation
    /terrain
    /particles
    /lighting
    /materials
    /shaders
    /scripting
    /ui
    /networking
    /input
    /assets
    /serialization
    /streaming
    /world
    /camera
    /plugins
    /profiling
    /debug
    /platform
    /build

/editor
    /viewport
    /hierarchy
    /inspector
    /content-browser
    /material-editor
    /shader-editor
    /animation-editor
    /terrain-editor
    /particle-editor
    /blueprint-editor
    /audio-editor
    /world-settings
    /project-settings
    /build-settings
    /profiler
    /console
    /debugger
    /ai-bridge

/project
    /assets
    /scenes
    /scripts
    /materials
    /shaders
    /textures
    /models
    /animations
    /audio
    /prefabs
    /particles
    /terrain
    /ui
    /plugins
    /settings
    /generated
    /cache

/tools
    /asset-importer
    /shader-compiler
    /build-tool
    /packager
    /profiler
    /asset-processor
```

Keep runtime systems independent from editor UI wherever possible.

The editor interacts with the engine through stable APIs and command systems rather than directly manipulating arbitrary internal state.

---

# 3. ENGINE CORE

Implement a proper Engine Core.

It should manage:

* engine initialization
* subsystem registration
* lifecycle
* frame loop
* fixed timestep
* variable timestep
* resource ownership
* memory management
* job scheduling
* asynchronous tasks
* event bus
* messaging
* dependency management
* engine services
* configuration
* logging
* crash handling
* profiling
* platform abstraction

Create clear lifecycle states:

```text
Boot
Initialize
LoadProject
LoadWorld
Running
Paused
Stopping
Shutdown
```

Implement a central EngineContext that exposes registered services without turning everything into tightly coupled global state.

---

# 4. ENTITY COMPONENT SYSTEM

Create a production-quality ECS.

Entities should be lightweight identifiers.

Components should contain data.

Systems operate over relevant component combinations.

Core components can include:

* Transform
* MeshRenderer
* SkinnedMeshRenderer
* Camera
* Light
* Rigidbody
* Collider
* CharacterController
* AudioSource
* AudioListener
* Script
* Animator
* ParticleEmitter
* NavigationAgent
* NavigationObstacle
* Terrain
* UIElement
* ReflectionProbe
* Decal
* PostProcessVolume
* NetworkIdentity
* Tag
* Layer

The ECS should support:

* entity creation
* destruction
* cloning
* parenting
* prefab relationships
* component dependencies
* runtime component addition/removal
* serialization
* editor reflection
* undo/redo
* property inspection
* scripting access
* AI access

Use stable UUIDs for persistent entities.

---

# 5. SCENE GRAPH AND WORLD SYSTEM

Create a proper hierarchical Scene Graph.

Each scene should support:

* world root
* folders
* parent/child relationships
* transforms
* nested objects
* reusable prefabs
* scene instances
* sub-scenes
* streaming levels
* world partitions
* hidden objects
* locked objects
* layers
* tags

Transform should support:

* local position
* local rotation
* local scale
* world position
* world rotation
* world scale
* matrices
* quaternion rotation

The editor Hierarchy panel must reflect the real scene graph.

Changes in the hierarchy must immediately modify real engine state.

---

# 6. LARGE WORLD SUPPORT

Design for large environments.

Include architecture for:

* world partitioning
* spatial partitioning
* chunk loading
* asynchronous streaming
* level-of-detail
* hierarchical LOD
* occlusion
* origin rebasing if required
* distance-based activation
* texture streaming
* mesh streaming
* terrain streaming

Do not load an entire large world into memory unnecessarily.

---

# 7. RENDERING ENGINE

Build a proper real-time rendering subsystem.

Do not hard-code rendering directly inside viewport UI components.

Create a Rendering Hardware Interface abstraction.

Architecture:

```text
Scene
↓
Render Extraction
↓
Render Graph
↓
Render Passes
↓
RHI
↓
Graphics API
↓
GPU
```

Depending on the platform, architect support for appropriate APIs such as:

* WebGPU for browser-based execution
* Vulkan
* DirectX 12
* Metal

Use platform-specific backends behind a common abstraction.

The renderer should support:

* forward rendering
* deferred rendering where applicable
* clustered lighting
* depth pre-pass
* shadow passes
* transparent pass
* sky pass
* post-processing
* UI pass
* debug pass

---

# 8. RENDER GRAPH

Implement a Render Graph.

Render passes should declare:

* inputs
* outputs
* textures
* buffers
* dependencies
* resource lifetimes

Allow render passes to be reordered and optimized safely.

Example:

```text
Depth Prepass
↓
GBuffer
↓
Shadow Pass
↓
Lighting
↓
Transparent Objects
↓
Volumetrics
↓
Post Processing
↓
UI
↓
Editor Overlays
```

---

# 9. PBR MATERIAL SYSTEM

Implement physically based rendering.

Default materials should support:

* Base Color
* Metallic
* Roughness
* Specular
* Normal
* Emissive
* Ambient Occlusion
* Opacity
* Opacity Mask
* Height
* Subsurface
* Clear Coat
* Clear Coat Roughness
* Anisotropy where supported

Support texture maps and constants.

Material parameters should be editable at runtime.

---

# 10. MATERIAL EDITOR

Create a node-based Material Editor.

Users should visually build materials.

Nodes should eventually cover areas such as:

* constants
* vectors
* textures
* UV coordinates
* time
* world position
* object position
* camera position
* normals
* vertex normals
* math
* interpolation
* noise
* fresnel
* gradients
* masks
* transforms
* procedural patterns
* material attributes

Allow connections into a final Material Output node.

Material graphs should compile into shaders.

Implement:

* shader compilation errors
* live preview
* parameter exposure
* material instances
* reusable node groups
* comments
* search
* node categories
* preview sphere/cube/plane

---

# 11. MATERIAL INSTANCES

Support parent materials and lightweight material instances.

For example:

```text
MasterCarPaint
├── RedCar
├── BlueCar
├── BlackCar
└── PoliceCar
```

Material instances should override parameters without recompiling the entire shader whenever possible.

---

# 12. SHADER ENGINE

Create a shader management system.

Support:

* vertex shaders
* fragment/pixel shaders
* compute shaders where available
* shader variants
* defines
* shader includes
* shader caching
* shader compilation
* hot reload
* compilation diagnostics

Create shader reflection so the editor knows available properties.

---

# 13. GLOBAL ILLUMINATION ARCHITECTURE

Provide a scalable lighting architecture.

Support combinations of:

* direct lighting
* indirect lighting
* environment lighting
* light probes
* reflection probes
* baked lighting
* real-time lighting
* screen-space effects

Design advanced real-time GI as a modular subsystem rather than tightly coupling the engine to one technique.

---

# 14. LIGHTING ENGINE

Support at minimum:

* Directional Light
* Point Light
* Spot Light
* Rect/Area Light
* Environment Light
* Sky Light
* Emissive contribution
* Reflection probes

Properties:

* intensity
* color
* temperature
* range
* attenuation
* shadow settings
* volumetric contribution
* cookie/profile where available
* channel/layer filtering

---

# 15. SHADOW SYSTEM

Create configurable shadows.

Support techniques appropriate to each platform.

Features:

* cascaded directional shadows
* point light shadows
* spotlight shadows
* contact shadows
* shadow bias
* normal bias
* shadow filtering
* configurable resolution
* shadow distance
* soft shadows
* cached shadows where useful

---

# 16. SKY AND ATMOSPHERE

Create real environment systems.

Support:

* physically based sky
* sun position
* atmosphere
* horizon
* fog
* height fog
* volumetric fog
* clouds architecture
* stars
* skybox
* HDRI environments
* ambient lighting

Users should be able to build day/night systems.

---

# 17. REFLECTION SYSTEM

Support:

* reflection probes
* box projection
* spherical probes
* environment maps
* screen-space reflections when available
* planar reflections for appropriate surfaces

---

# 18. POST-PROCESSING

Create post-processing volumes and camera effects.

Support:

* exposure
* auto exposure
* tone mapping
* bloom
* vignette
* chromatic aberration
* depth of field
* motion blur
* film grain
* LUT/color grading
* contrast
* saturation
* white balance
* sharpen
* ambient occlusion
* screen-space reflections
* fog integration

Effects should be configurable globally or through world volumes.

---

# 19. CAMERA ENGINE

Camera component should support:

* perspective
* orthographic
* FOV
* near plane
* far plane
* aspect ratio
* exposure
* post-processing
* viewport rectangle
* camera priority
* multiple cameras

Provide common camera systems:

* first person
* third person
* orbit
* top-down
* side-scroller
* cinematic
* spline camera

---

# 20. PHYSICS ENGINE

Integrate or build around a serious physics backend.

Keep the physics API abstract enough that the backend can be changed later.

Support:

* static rigid bodies
* dynamic rigid bodies
* kinematic rigid bodies
* gravity
* forces
* impulses
* torque
* mass
* drag
* angular drag
* sleeping
* collision detection
* continuous collision detection
* collision layers
* triggers
* raycasts
* sphere casts
* box casts
* overlap tests

Colliders:

* Box
* Sphere
* Capsule
* Convex Mesh
* Mesh Collider
* Heightfield

Physics events:

```text
OnCollisionEnter
OnCollisionStay
OnCollisionExit
OnTriggerEnter
OnTriggerStay
OnTriggerExit
```

---

# 21. PHYSICS JOINTS

Support:

* fixed joint
* hinge joint
* spring joint
* distance joint
* slider joint
* configurable joint

Design extension points for:

* vehicles
* ragdolls
* ropes
* destructible objects
* cloth
* soft bodies

---

# 22. CHARACTER CONTROLLER

Create a production-ready Character Controller.

Capabilities:

* walking
* running
* jumping
* crouching
* slopes
* steps
* ground detection
* air control
* gravity
* collision
* moving platforms

Do not force users to manually build basic character movement from raw physics APIs.

---

# 23. ANIMATION SYSTEM

Support:

* skeletal animation
* animation clips
* skinning
* blending
* animation layers
* additive animation
* animation events
* root motion
* animation curves
* morph targets
* blend shapes

---

# 24. ANIMATION STATE MACHINE

Create a visual animation graph.

Example:

```text
Idle
↓
Walk
↓
Run
↓
Jump
↓
Fall
↓
Land
```

Transitions should support conditions such as:

```text
speed > 0.1
isGrounded == false
jumpPressed == true
```

Allow blending between states.

---

# 25. BLEND TREES

Support:

* 1D blend trees
* 2D blend trees

Example:

```text
Speed
0 = Idle
0.5 = Walk
1 = Run
```

---

# 26. INVERSE KINEMATICS

Architecture should allow IK for:

* feet
* hands
* weapon holding
* character interaction
* aim systems
* procedural animation

---

# 27. CINEMATIC SYSTEM

Create a timeline/sequencer system.

Allow:

* camera cuts
* camera movement
* animation tracks
* object transforms
* audio
* events
* material parameters
* lighting
* particles
* post processing
* subtitles
* gameplay triggers

Users should be able to create cinematics without writing code.

---

# 28. PARTICLE AND VFX ENGINE

Create a GPU-friendly particle architecture.

Particles should support:

* spawn rate
* lifetime
* velocity
* acceleration
* gravity
* scale
* color
* rotation
* collision
* texture
* sprite sheets
* trails
* forces
* attraction
* turbulence
* noise

Create a node/graph-based advanced VFX architecture.

Possible systems:

```text
Emitter
↓
Spawn
↓
Initialize Particle
↓
Forces
↓
Collision
↓
Update
↓
Renderer
```

---

# 29. TERRAIN ENGINE

Support:

* terrain creation
* heightmaps
* sculpt
* smooth
* flatten
* raise/lower
* erosion architecture
* painting
* multiple terrain layers
* texture blending
* grass
* trees
* foliage
* rocks
* procedural scatter
* terrain LOD
* terrain collision

Terrain changes must affect the real runtime world.

---

# 30. FOLIAGE SYSTEM

Support high-performance instancing for:

* grass
* trees
* bushes
* rocks
* props

Include:

* density
* random scale
* random rotation
* slope filtering
* altitude filtering
* paint/erase
* procedural scatter
* culling
* LOD
* GPU instancing

---

# 31. NAVIGATION SYSTEM

Create AI navigation tools.

Support:

* navmesh generation
* navmesh visualization
* navigation agents
* obstacles
* dynamic obstacles
* pathfinding
* destination setting
* path recalculation
* navigation links
* agent radius
* agent height
* step height
* slope limits

Expose navigation to scripts and AI-generated gameplay logic.

---

# 32. GAMEPLAY AI

Provide architecture for gameplay behaviors.

Possible tools:

* behavior trees
* state machines
* blackboards
* perception
* vision
* hearing
* target selection
* patrol
* chase
* attack
* flee
* cover

Do not mix gameplay AI with Bhippi's coding assistant AI. They are separate systems.

---

# 33. AUDIO ENGINE

Support:

* WAV
* MP3 where legally/technically supported
* OGG
* streaming audio
* positional audio
* 3D attenuation
* looping
* pitch
* volume
* spatialization
* audio buses
* audio groups
* mixers
* filters
* reverb
* effects
* listener components

Create an audio mixer/editor.

---

# 34. INPUT SYSTEM

Build an action-based Input System.

Instead of hardcoding keys:

```text
Jump → Space / Gamepad A
MoveForward → W / Left Stick
Fire → Mouse1 / Right Trigger
```

Support:

* keyboard
* mouse
* controller/gamepad
* touch
* gestures

Architecture should allow future:

* VR controllers
* custom controllers

---

# 35. SCRIPTING SYSTEM

Game logic must be scriptable.

Provide a stable API.

Depending on Bhippi's implementation stack, choose suitable scripting technologies while keeping the architecture language-agnostic.

Scripts must be able to:

* query entities
* access components
* spawn entities
* destroy entities
* manipulate transforms
* control animation
* access physics
* play audio
* control UI
* raycast
* access input
* load assets
* switch scenes
* create timers
* subscribe to events

Example conceptual API:

```ts
const player = world.findEntity("Player");

const transform = player.getComponent(Transform);
const body = player.getComponent(RigidBody);

body.addForce(direction.multiply(10));
```

---

# 36. SCRIPT HOT RELOAD

Whenever possible, code changes should hot reload without restarting the entire editor.

Provide clear failure handling when hot reload is impossible.

---

# 37. VISUAL SCRIPTING SYSTEM

Create Bhippi Graph, a Blueprint-style visual scripting environment.

This must be Bhippi's own implementation.

Nodes should represent:

* events
* functions
* variables
* conditions
* loops
* math
* transforms
* physics
* audio
* animation
* UI
* input
* spawning
* timers
* networking

Example:

```text
On Game Start
      ↓
Spawn Enemy
      ↓
Wait 5 Seconds
      ↓
Spawn Enemy
```

Another:

```text
On Trigger Enter
      ↓
Check Tag = Player
      ↓
Open Door
```

Visual graphs must compile or execute through real runtime logic.

---

# 38. PREFAB SYSTEM

Create reusable prefab assets.

Example:

```text
Enemy.prefab
├── Mesh
├── Rigidbody
├── Collider
├── Animator
├── EnemyAI
└── Health
```

Prefab modifications should propagate appropriately.

Allow instance overrides.

Support nested prefabs.

---

# 39. ASSET SYSTEM

Build an Asset Database.

Every imported asset receives:

* asset UUID
* metadata
* source path
* imported representation
* dependencies
* type
* import settings
* thumbnail
* version/cache information

Example:

```text
player.fbx
player.fbx.meta
```

Never rely exclusively on filenames as asset identity.

---

# 40. CONTENT BROWSER

Create a professional Content Browser.

Support:

* folders
* search
* filtering
* drag and drop
* thumbnails
* asset types
* rename
* duplicate
* move
* delete
* references
* favorites
* recently used assets

Asset categories:

* Scenes
* Models
* Materials
* Textures
* Animations
* Audio
* Prefabs
* Scripts
* Shaders
* VFX
* UI
* Terrain
* Plugins

---

# 41. ASSET IMPORT PIPELINE

Support common formats.

Models:

* glTF
* GLB
* FBX through an appropriate importer
* OBJ where useful

Textures:

* PNG
* JPG
* WebP
* HDR
* EXR where platform allows
* compressed GPU texture formats

Audio:

* WAV
* OGG
* MP3 where supported

Animations:

* skeletal animation
* glTF animation
* FBX animation where supported

Automatically process:

* normals
* tangents
* UV data
* texture compression
* mipmaps
* mesh optimization
* animation metadata

---

# 42. RESOURCE MANAGER

Never randomly load assets everywhere.

Create ResourceManager APIs:

```text
load
loadAsync
retain
release
preload
unload
```

Use caching and reference tracking.

---

# 43. ASYNCHRONOUS LOADING

Asset importing, shader compilation, scene streaming, texture loading, and large resource processing should not freeze the editor.

Use workers/jobs/background processing appropriately.

Show progress inside the editor.

---

# 44. MEMORY MANAGEMENT

Track resource memory.

Profiler should show:

* texture memory
* mesh memory
* animation memory
* audio memory
* render target memory
* CPU memory
* GPU memory where accessible

---

# 45. GAME UI SYSTEM

Create a runtime UI framework.

Support:

* Canvas
* Panel
* Text
* Image
* Button
* Slider
* Checkbox
* Dropdown
* Scroll View
* Progress Bar
* Input
* Layout containers
* Anchors
* responsive layout

Game UI should work independently from the Bhippi editor UI.

---

# 46. UI EDITOR

Create drag-and-drop UI layout editing.

Support:

* anchors
* alignment
* margins
* padding
* flex/grid concepts where useful
* responsive preview
* resolution preview
* safe zones

---

# 47. DEBUG DRAW SYSTEM

Support viewport debug overlays:

* bounding boxes
* colliders
* normals
* tangents
* navmesh
* light bounds
* camera frustums
* audio ranges
* physics contacts
* paths
* skeletons

---

# 48. EDITOR VIEWPORT

Create a real engine viewport.

It should render the actual scene through the engine renderer.

Viewport modes:

* Lit
* Unlit
* Wireframe
* Lighting Only
* Normal
* Depth
* Collision
* Navigation
* Overdraw/debug
* Shader complexity architecture where feasible

Viewport controls:

* orbit
* pan
* zoom
* fly
* frame selected
* focus
* camera speed
* perspective/orthographic
* snapping
* local/global coordinates

---

# 49. TRANSFORM GIZMOS

Create professional gizmos:

* Move
* Rotate
* Scale
* Universal Transform

Support:

* X axis
* Y axis
* Z axis
* plane handles
* world space
* local space
* snapping
* numeric input

---

# 50. OBJECT SELECTION

Support:

* click selection
* rectangle selection
* multi-select
* shift select
* hierarchy select
* selection outlines
* locked entities
* hidden entities

Selection must be synchronized across:

```text
Viewport
Hierarchy
Inspector
AI context
```

---

# 51. INSPECTOR

The inspector must be generated from actual component/property metadata.

Do not manually create special UI for every property.

Use a reflection/property schema system.

Support editors for:

* numbers
* vectors
* colors
* enums
* booleans
* strings
* asset references
* entity references
* curves
* gradients

AI-generated components should be capable of exposing properties through metadata.

---

# 52. UNDO / REDO

Every editor-changing action should support command-based undo/redo wherever practical.

Examples:

```text
Move Entity
Delete Entity
Create Entity
Change Material
Modify Component
Rename Entity
Change Lighting
Import Asset
```

Use a centralized EditorCommand architecture.

---

# 53. SAVE SYSTEM

Projects must actually save.

Serialize:

* scenes
* entities
* transforms
* components
* material assignments
* prefab links
* scripts
* settings
* references
* editor metadata where necessary

Use stable versioned formats.

Do not silently corrupt projects when formats evolve.

---

# 54. AUTO SAVE

Provide configurable autosave.

Maintain recovery snapshots when possible.

---

# 55. PROJECT SYSTEM

A Bhippi game project should contain metadata such as:

```json
{
  "name": "MyGame",
  "engineVersion": "...",
  "startupScene": "...",
  "renderSettings": {},
  "physicsSettings": {},
  "inputSettings": {},
  "buildSettings": {}
}
```

---

# 56. PROJECT SETTINGS

Create real settings panels for:

* General
* Rendering
* Physics
* Audio
* Input
* Navigation
* Layers
* Tags
* Time
* Quality
* Build
* Plugins
* Networking
* AI access

---

# 57. PLAY MODE

The editor must have:

* Edit
* Play
* Pause
* Step

When Play starts:

1. create runtime world state
2. execute scripts
3. enable physics
4. enable animation
5. process gameplay
6. use correct camera
7. capture runtime logs

Stopping Play Mode should return safely to editor state.

Optionally support applying selected runtime changes back to edit state.

---

# 58. GAME VIEW

Separate:

```text
Scene View
Game View
```

Scene View = development.

Game View = player camera output.

---

# 59. LOGGING AND CONSOLE

Create proper log levels:

```text
Trace
Debug
Info
Warning
Error
Fatal
```

Console should support:

* search
* categories
* timestamps
* source file
* stack traces
* clickable code links
* clearing
* filtering

The AI should be able to inspect this console.

---

# 60. DEBUGGER

Allow debugging of gameplay/scripts.

Features:

* breakpoints where runtime supports them
* stack traces
* variable inspection
* runtime entity inspection
* pause
* step
* continue

---

# 61. PROFILER

Create a real profiler.

Show:

CPU:

* frame time
* update
* scripting
* physics
* animation
* render preparation
* audio
* AI/navigation

GPU:

* rendering passes
* draw calls
* triangles
* shader time where available
* shadows
* post process

Memory:

* textures
* meshes
* audio
* scripts
* scene
* allocations

---

# 62. FRAME DEBUGGER

Add architecture for inspecting the current rendered frame pass-by-pass.

Users should be able to see what was rendered and which passes contributed.

---

# 63. PERFORMANCE SYSTEMS

Implement or prepare:

* frustum culling
* occlusion culling
* object pooling
* mesh batching
* GPU instancing
* LOD
* texture compression
* mipmapping
* async loading
* draw call reduction
* shader caching
* asset streaming

---

# 64. PLUGIN ARCHITECTURE

The engine must not become one huge monolith.

Plugins/modules should be able to register:

* components
* systems
* editor panels
* asset types
* importers
* exporters
* menus
* commands
* AI tools
* render passes
* scripting APIs

Plugins need:

* manifests
* version requirements
* dependencies
* enable/disable state
* error isolation where practical

---

# 65. EDITOR EXTENSIBILITY

Allow developers to create custom:

* inspectors
* windows
* components
* tools
* gizmos
* importers
* graphs
* editor actions

---

# 66. NETWORKING ARCHITECTURE

Prepare real multiplayer architecture.

Support abstractions for:

* server
* clients
* sessions
* replicated entities
* remote procedure calls
* state synchronization
* ownership
* interpolation
* prediction
* reconciliation

Do not tightly couple offline gameplay code to networking.

---

# 67. BUILD SYSTEM

Projects need to compile/package into runnable applications.

Create:

```text
Development Build
Debug Build
Release Build
```

Build pipeline should:

1. validate project
2. resolve assets
3. compile shaders
4. compile scripts
5. optimize assets
6. remove unused editor resources
7. package runtime
8. generate target application

Targets can expand over time:

* Web
* Windows
* macOS
* Linux
* Android
* iOS

Only advertise targets that are actually working.

---

# 68. HEADLESS RUNTIME

Create a runtime executable/process that can run without the editor.

Important for:

* dedicated servers
* automated testing
* CI
* AI testing
* build verification

---

# 69. AUTOMATED TESTING

Create tests at different levels.

Unit tests:

* math
* transforms
* ECS
* serialization
* assets

Integration tests:

* scene loading
* physics
* scripting
* rendering pipeline

Editor tests:

* undo/redo
* asset import
* hierarchy
* inspector

Regression tests should protect major systems.

---

# 70. CRASH RECOVERY

Protect project data.

Implement:

* autosave
* recovery states
* safe writing
* temporary files
* crash logs
* corrupted resource detection

---

# 71. VERSIONING

Scenes and project files must include schema versions.

Provide migrations when formats change.

Example:

```text
SceneVersion 4 → SceneVersion 5
```

Do not break older Bhippi projects silently.

---

# 72. SOURCE CONTROL FRIENDLY DATA

Prefer deterministic, readable serialization formats for editor metadata where performance permits.

Avoid unnecessary file changes.

Provide sensible handling for:

* Git
* branches
* conflicts
* binary assets

---

# 73. EDITOR UI LAYOUT

Bhippi Creator's game engine workspace should feel professional but should still follow Bhippi's own design language.

Recommended layout:

```text
┌─────────────────────────────────────────────────────────────┐
│ Main Toolbar                                                │
├──────────────┬────────────────────────────┬─────────────────┤
│              │                            │                 │
│ Hierarchy    │        3D VIEWPORT         │    Inspector    │
│              │                            │                 │
│              │                            │                 │
├──────────────┴────────────────────────────┴─────────────────┤
│ Content Browser / Console / Timeline / Animation / AI       │
└─────────────────────────────────────────────────────────────┘
```

Main toolbar:

* Select
* Move
* Rotate
* Scale
* Local/World
* Snapping
* Play
* Pause
* Stop
* Build
* Camera
* View options

Keep it visually clean.

Do not dump hundreds of controls directly onto the screen.

Use contextual panels and searchable command palettes.

---

# 74. EDITOR DOCKING SYSTEM

Major editor windows should support:

* docking
* tabs
* resizing
* splitting
* opening/closing
* restoring layouts

Save workspace configurations.

Potential tabs:

```text
Viewport
Hierarchy
Inspector
Content
Material
Animation
VFX
Blueprint
Profiler
Console
AI
```

---

# 75. COMMAND PALETTE

Create an editor command architecture.

Example commands:

```text
entity.create
entity.delete
entity.duplicate
component.add
scene.save
scene.load
asset.import
material.create
script.create
game.play
game.stop
build.start
viewport.focus
```

This command architecture is extremely important because it will also serve the AI integration.

---

# 76. THE MOST IMPORTANT SYSTEM: AI ↔ ENGINE BRIDGE

Bhippi Creator is an AI-native engine.

This is what should differentiate it from ordinary game engines.

Do NOT let the AI control the engine by randomly clicking UI coordinates.

Create a structured **AI Engine Bridge**.

The AI needs tools/APIs such as:

```text
getProjectInfo()
getSceneTree()
getSelection()
getEntity(id)
findEntities(query)
createEntity(type)
deleteEntity(id)
duplicateEntity(id)

addComponent(entity, type)
removeComponent(entity, type)
getComponent(entity, type)
setComponentProperty(entity, component, property, value)

setTransform(entity, transform)
parentEntity(child, parent)

createScene()
loadScene()
saveScene()

importAsset(path)
searchAssets(query)
createMaterial()
setMaterial(entity, material)
createTexture()

createLight(type)
createCamera()

runGame()
pauseGame()
stopGame()

getConsoleErrors()
getRuntimeErrors()
getProfilerSnapshot()
takeViewportSnapshot()

createScript()
editScript()
runTests()
buildProject()
```

These APIs should operate against actual engine state.

---

# 77. ENGINE SCHEMA FOR AI

The AI needs to understand every supported engine object.

Expose machine-readable schema.

Example:

```json
{
  "component": "PointLight",
  "properties": {
    "color": {
      "type": "color"
    },
    "intensity": {
      "type": "number",
      "min": 0
    },
    "range": {
      "type": "number",
      "min": 0
    },
    "castShadows": {
      "type": "boolean"
    }
  }
}
```

Do this automatically using the same reflection system powering the Inspector.

This prevents AI hallucination.

---

# 78. AI SCENE UNDERSTANDING

When asked about a scene, AI should receive structured context.

Example:

```text
Current Scene: Warehouse.scene

Entities: 184

Selected:
Player

Player:
Transform
CharacterController
Animator
PlayerController
Health

Nearby:
CameraRig
Weapon
Door_03
Enemy_12

Errors:
PlayerController.ts:182
Null reference: equippedWeapon
```

Do not dump the entire project into the context window unnecessarily.

Use retrieval.

---

# 79. AI VISUAL UNDERSTANDING

The AI must be able to inspect the viewport visually.

Implement a controlled system that can capture:

* viewport image
* active game camera
* selected object view
* material preview
* UI preview

This visual information can be supplied to multimodal AI providers when supported.

Therefore the AI can identify issues like:

"the character is floating above the ground."

Then inspect:

* Transform
* Collider
* CharacterController
* terrain position

And fix the issue through engine APIs.

---

# 80. AI READ → PLAN → ACT → VERIFY LOOP

AI actions should follow:

```text
Understand
↓
Inspect Project
↓
Inspect Scene
↓
Create Plan
↓
Perform Engine Commands
↓
Run/Preview
↓
Inspect Result
↓
Check Errors
↓
Fix
↓
Verify
↓
Complete
```

Never consider an engine operation complete just because code was written.

Verification is essential.

---

# 81. AI TRANSACTION SYSTEM

Group AI changes together.

Example:

```text
Transaction:
"Create forest environment"

Actions:
- create terrain
- import textures
- create material
- paint terrain
- scatter trees
- add directional light
- create sky
```

User should be able to:

```text
Undo AI Change
```

in one operation.

---

# 82. AI CHANGE PREVIEW

For destructive or substantial operations, allow the engine to show the AI's plan.

Example:

```text
AI wants to:
+ Create 18 entities
+ Modify WorldSettings
+ Change DirectionalLight
+ Import 7 assets
- Remove OldSky
```

Depending on user permission mode, Bhippi can:

* ask first
* execute automatically
* execute fully autonomously

---

# 83. AI ENGINE EVENT STREAM

The assistant needs continuous structured signals while executing work.

Example:

```json
{
  "event": "entity_created",
  "entity": "Enemy_04",
  "id": "..."
}
```

Other events:

```text
asset_imported
scene_loaded
shader_compiled
script_compiled
game_started
runtime_error
build_completed
physics_error
```

This allows autonomous agents to understand what actually happened.

---

# 84. AI ERROR RECOVERY

If the engine reports:

```text
Shader compilation failed
```

the AI should:

1. retrieve compiler output
2. locate shader/material
3. correct it
4. recompile
5. inspect preview
6. verify error disappears

If script crashes:

1. obtain exception
2. inspect stack
3. inspect relevant source
4. patch
5. restart/reload
6. verify

---

# 85. AI SHOULD UNDERSTAND ENGINE DOCUMENTATION

Generate structured internal documentation for:

* every engine subsystem
* every component
* every API
* every editor command
* every scripting interface
* every graph node
* every property

Keep this documentation synchronized with source code whenever possible.

AI agents should retrieve this documentation before inventing APIs.

---

# 86. AI CONTEXT MANAGER

Do not send the entire game project to the model.

Create contextual retrieval.

For example:

User:

"Make this door open when the player comes close."

AI automatically gathers:

```text
Selected Door
Door hierarchy
Door collider
Door scripts
Player tag
Player controller
Nearby trigger volumes
Relevant scripting API
```

Then performs the operation.

---

# 87. MULTI-AGENT SUPPORT

Because Bhippi Creator supports multiple AI providers, separate the AI provider layer from the engine.

Provider adapters:

```text
OpenAI
Anthropic
Gemini
Grok
Local Models
Other providers
```

should all communicate with the same Bhippi Agent Protocol.

Architecture:

```text
AI Provider
      ↓
Agent Runtime
      ↓
Tool Router
      ↓
Bhippi Engine Bridge
      ↓
Engine
```

Never make the engine dependent on a specific AI provider.

---

# 88. AI CAPABILITY PERMISSIONS

Engine AI tools need permissions.

Categories:

```text
READ_PROJECT
READ_SCENE
READ_FILES
EDIT_SCENE
EDIT_ASSETS
EDIT_FILES
RUN_PROJECT
RUN_TERMINAL
BUILD_PROJECT
DELETE_ASSETS
CHANGE_SETTINGS
```

Permission levels:

```text
Ask
Allow This Session
Always Allow
Deny
```

---

# 89. ENGINE LOCKING

If multiple agents operate simultaneously, prevent destructive race conditions.

Example:

Agent A modifying `PlayerController`.

Agent B wants to modify the same file.

The orchestrator should detect the conflict.

Use scoped locks or transactions.

---

# 90. AI ACTION HISTORY

Maintain a record like:

```text
10:32 Claude
Created EnemySpawner

10:34 Codex
Modified PlayerController

10:36 Gemini
Created 4 materials
```

Allow inspecting:

* changes
* diffs
* engine operations
* errors
* result

---

# 91. AI CAN CONTROL THE EDITOR VIEW

Provide semantic viewport operations:

```text
focusEntity(id)
frameSelection()
setCameraPosition()
setViewportMode()
openMaterial(asset)
openScript(asset)
openAnimation(asset)
```

Do not rely primarily on simulated mouse clicks.

---

# 92. AI CAN UNDERSTAND USER SELECTION

If user clicks:

```text
Enemy_03
```

and says:

"Make him twice as strong and red."

The AI should automatically know the selected object.

It should inspect:

* health component
* material

Then make minimal changes.

---

# 93. AI CAN BUILD COMPLETE SYSTEMS

Example request:

"Create an enemy that patrols between these three points and chases me if I come within 8 meters."

AI should:

1. inspect selected points
2. create/reuse enemy entity
3. create nav agent
4. create patrol logic
5. configure detection radius
6. create chase state
7. connect animator
8. run navigation bake if needed
9. start game
10. inspect behavior
11. fix errors

---

# 94. AI PLAYTESTING

Create support for automated playtesting.

AI should be able to:

* launch game
* inspect game state
* send controlled input
* move player
* inspect logs
* capture frames
* restart
* execute test scenarios

Examples:

```text
Walk forward for 3 seconds.
Jump.
Shoot enemy.
Verify enemy health decreased.
```

---

# 95. ENGINE STATE SNAPSHOTS

For AI testing/debugging expose:

```json
{
  "player": {
    "position": [0, 1, 12],
    "health": 76,
    "grounded": true
  },
  "enemyCount": 7,
  "fps": 115
}
```

Do not force AI to infer everything from screenshots.

Combine structured state + visual state.

---

# 96. WORLD QUERY LANGUAGE

Create a powerful query API.

Examples:

```text
type:Light
tag:Enemy
name:"Door"
component:Rigidbody
within:10m of Player
layer:Environment
```

AI and editor search can share this.

---

# 97. ENGINE COMMAND RESPONSE FORMAT

Every AI command should return structured results.

Example:

```json
{
  "success": true,
  "operation": "entity.create",
  "entity": {
    "id": "uuid",
    "name": "Enemy_04"
  }
}
```

Errors:

```json
{
  "success": false,
  "code": "COMPONENT_NOT_FOUND",
  "message": "CharacterController does not exist on Player"
}
```

Never return vague failures.

---

# 98. GENERATED CONTENT TRACKING

When AI generates:

* scripts
* scenes
* materials
* shaders
* prefabs
* UI
* assets

record metadata.

Example:

```text
GeneratedBy: Claude
CreatedAt: ...
PromptTask: "Create enemy"
```

Do not interfere with exported runtime builds unless explicitly required.

---

# 99. PROCEDURAL GENERATION

Prepare tools for AI/procedural systems to generate:

* environments
* terrain
* cities
* dungeons
* foliage
* rooms
* level layouts
* roads
* object arrangements

Generation should result in ordinary editable engine entities/assets.

---

# 100. EDITOR TOOL MODE

Allow specialized tools:

```text
Terrain
Foliage
Mesh Paint
Spline
Decal
Navigation
Lighting
Physics
Animation
VFX
UI
```

---

# 101. SPLINE SYSTEM

Create reusable spline tools for:

* roads
* rivers
* camera paths
* object paths
* fences
* cables

Support control points and tangent editing.

---

# 102. DECAL SYSTEM

Support projected decals for:

* bullet holes
* dirt
* graffiti
* cracks
* blood where users choose to create such game content
* environment detail

---

# 103. MESH TOOLS

Provide useful editor mesh operations:

* create primitive
* generate normals
* generate tangents
* flip normals
* basic UV tools
* merge meshes
* separate where feasible
* collider generation
* LOD generation

For advanced modeling, architecture can later support dedicated modeling tools.

---

# 104. PRIMITIVES

Default engine primitives:

```text
Cube
Sphere
Plane
Capsule
Cylinder
Cone
Quad
```

Each should create proper renderable engine entities.

---

# 105. TEXTURE TOOLS

Provide import settings:

* compression
* filtering
* wrap
* sRGB
* normal map mode
* mipmaps
* maximum size
* texture type

---

# 106. QUALITY LEVELS

Create scalable presets:

```text
Low
Medium
High
Ultra
Custom
```

Control:

* shadow resolution
* render scale
* LOD distance
* post-processing
* volumetrics
* texture quality
* effects
* particles

---

# 107. RENDER RESOLUTION

Support:

* native resolution
* resolution scaling
* dynamic resolution architecture
* anti-aliasing options appropriate to renderer

Keep future upscaling integrations modular.

---

# 108. EDITOR PERFORMANCE

The editor itself should not constantly rerender unrelated UI.

Separate engine simulation from application UI state.

Avoid unnecessary React/UI framework rerenders if the shell uses React.

The viewport should have its own rendering lifecycle.

---

# 109. WORKER/JOB SYSTEM

Implement task scheduling for expensive work:

* asset import
* shader compilation
* terrain processing
* navigation baking
* mesh processing
* build packaging

Avoid blocking the primary editor thread.

---

# 110. EVENT SYSTEM

Systems communicate through well-defined events rather than deeply coupled direct calls.

Example:

```text
SceneLoaded
EntityCreated
EntityDestroyed
ComponentChanged
AssetImported
ScriptCompiled
GameStarted
GameStopped
```

---

# 111. ENGINE SERVICE REGISTRY

Services can include:

```text
RenderService
PhysicsService
AudioService
AssetService
SceneService
NavigationService
InputService
ScriptService
BuildService
AIEngineBridge
```

Avoid turning every class into a singleton.

---

# 112. LOGICAL THREADING MODEL

Plan thread ownership carefully.

Conceptually:

```text
Main/Game Thread
Render Thread where architecture permits
Worker Pool
Asset Workers
Shader Compilation Workers
Physics jobs where backend permits
```

Do not introduce threading blindly.

Correctness is more important than pretending to be multithreaded.

---

# 113. DATA ORIENTED PERFORMANCE

For systems processing thousands of entities, prefer efficient contiguous data/storage strategies rather than deeply nested object graphs.

Especially:

* transforms
* particles
* rendering
* physics mappings
* animations

---

# 114. ENGINE API STABILITY

Public APIs should be versioned and documented.

Keep internal implementation replaceable.

For example:

```text
Bhippi.Renderer
```

should not expose raw WebGPU implementation details everywhere.

---

# 115. DO NOT BUILD A FAKE ENGINE

Reject these shortcuts:

* buttons with no implementation
* panels containing hardcoded placeholder data
* "physics" implemented by manually changing Y coordinates
* fake profiler numbers
* fake build progress
* fake asset importer
* fake node editor
* hardcoded scene hierarchy
* fake material preview
* screenshots masquerading as a viewport
* one giant React state object containing the entire engine
* one file containing the entire engine
* AI only changing UI text instead of actual scene data
* pretending features work when backend support does not exist

If a feature is incomplete, mark it internally as incomplete.

Do not pretend.

---

# 116. IMPLEMENT IN LAYERS

Do not attempt to make every feature simultaneously without foundations.

Build in logical stages.

## Phase 1 — Foundation

Create:

* engine core
* math
* event system
* ECS
* transform
* scene graph
* serialization
* project system
* resource IDs

## Phase 2 — Renderer

Create:

* graphics abstraction
* viewport
* meshes
* camera
* textures
* basic PBR
* lights
* shadows

## Phase 3 — Editor

Create:

* viewport tools
* hierarchy
* inspector
* content browser
* transform gizmos
* undo/redo

## Phase 4 — Assets

Create:

* importer
* asset database
* thumbnails
* caching
* async loading

## Phase 5 — Gameplay

Create:

* scripts
* physics
* input
* animation
* prefabs
* runtime UI

## Phase 6 — Advanced Rendering

Create:

* material graph
* post processing
* reflections
* atmosphere
* volumetrics
* advanced lighting

## Phase 7 — World Tools

Create:

* terrain
* foliage
* navigation
* splines
* decals

## Phase 8 — Advanced Gameplay

Create:

* behavior systems
* visual scripting
* cinematics
* particles
* audio editor

## Phase 9 — Production

Create:

* profiler
* debugger
* build system
* platform deployment
* networking architecture
* plugin SDK

## Phase 10 — AI-Native Engine

Fully connect:

* engine commands
* project schema
* scene schema
* selection context
* viewport screenshots
* console
* playtesting
* AI transactions
* multi-agent coordination

---

# 117. EACH PHASE MUST ACTUALLY WORK

At the end of every phase:

* compile
* run
* test
* verify
* document

Never leave ten partially implemented systems merely to claim a larger feature list.

---

# 118. DEFINITION OF DONE FOR FEATURES

A feature is not complete merely because its UI exists.

For each engine feature verify:

```text
Backend exists
↓
Runtime implementation exists
↓
Editor interface exists
↓
Serialization works
↓
Undo/redo works where appropriate
↓
Scripting API exists where relevant
↓
AI API exists where relevant
↓
Errors are handled
↓
Tests exist
↓
Documentation exists
```

---

# 119. DEVELOPMENT RULES

Whenever modifying the current Bhippi codebase:

1. inspect the existing architecture first
2. understand existing systems
3. identify reusable code
4. do not unnecessarily rewrite working systems
5. create a migration plan
6. preserve existing user projects
7. keep modules separated
8. avoid circular dependencies
9. maintain strict typing
10. document major APIs
11. test every subsystem
12. remove dead temporary code
13. never ship fake implementation
14. keep editor and runtime separated
15. ensure AI has structured access to new features

---

# 120. SECURITY

Because AI agents and scripts can execute actions, create boundaries.

Do not allow arbitrary project scripts to silently gain access to:

* host filesystem outside project permissions
* credentials
* AI API keys
* unrelated user files
* operating system commands

Keep sensitive credentials inside secure application configuration.

Never serialize provider keys into project files.

---

# 121. SANDBOXING

Where possible separate:

```text
Editor
Runtime
User Game Scripts
AI Agent
External Tools
```

with explicit permission boundaries.

---

# 122. PROJECT VALIDATION

Before running or building, detect:

* missing assets
* missing components
* broken references
* invalid scripts
* shader failures
* invalid startup scene
* duplicate identifiers
* dependency problems

Expose this validation to the AI.

---

# 123. ENGINE HEALTH PANEL

Create a Development/Health panel showing:

```text
Renderer ✓
Physics ✓
Audio ✓
Scripts ✓
Navigation ✓
Assets ✓
Shaders ✓
AI Bridge ✓

Warnings: 3
Errors: 0
```

This information must come from real subsystem health checks.

---

# 124. SEARCH EVERYTHING

Create global engine search.

User should be able to search:

```text
Player
```

and see:

* entities
* scripts
* prefabs
* materials
* assets
* components
* project settings references

AI should use the same index.

---

# 125. REFERENCES VIEWER

Allow users and AI to answer:

"What uses this material?"

"What references this texture?"

"What scenes use this prefab?"

Build asset/entity dependency tracking.

---

# 126. COMMANDS SHOULD BE SCRIPTABLE

Editor commands should be callable from:

* UI
* keyboard shortcut
* internal automation
* AI
* editor scripting

One behavior, multiple interfaces.

---

# 127. KEEP HUMAN AND AI OPERATIONS IDENTICAL

For example:

Human:

drags Point Light into viewport.

AI:

calls:

```text
createEntity(PointLight)
```

Both should ultimately execute the same engine command.

Do not maintain separate fake AI state.

---

# 128. AI PROJECT MEMORY

Bhippi should maintain structured project knowledge.

Examples:

```text
Player prefab = /Assets/Characters/Player.prefab
Main menu = /Assets/UI/MainMenu
Enemy base class = EnemyBase
Main level = City01
Primary art style = Stylized PBR
```

Use project-level memory rather than forcing the AI to rediscover everything on every message.

Memory must be inspectable and correctable.

---

# 129. AI SHOULD DETECT CURRENT WORKSPACE

If the user is inside:

* Material Editor

then:

"Make it shinier."

should refer to the active material.

If user is inside:

* Animation Editor

then:

"Make this transition faster."

should refer to the selected transition.

If user is inside:

* Scene View

then contextual scene/selection information should be available automatically.

---

# 130. CONTEXT GRAPH

Maintain a lightweight context graph:

```text
CurrentProject
    ↓
CurrentScene
    ↓
CurrentSelection
    ↓
CurrentAsset
    ↓
CurrentEditor
    ↓
CurrentRuntimeState
```

Feed relevant portions to AI.

---

# 131. AI RESPONSE WITH ACTIONS

When AI performs work, chat can display semantic actions:

```text
✓ Created Player
✓ Added CharacterController
✓ Created FollowCamera
✓ Imported animations
✓ Created locomotion state machine
✓ Started play test
✓ Fixed collision issue
✓ Verified movement
```

These statuses must correspond to actual executed commands.

---

# 132. LIVE AGENT STATUS

Bhippi Creator can show AI workers doing engine work.

Example:

```text
Claude
Editing PlayerController

Codex
Building terrain shader

Gemini
Checking runtime errors
```

Every worker needs clear ownership and task state.

---

# 133. MULTI-AGENT ENGINE OPERATIONS

Agents can divide tasks.

Example request:

"Build a third-person forest game."

Planner:
Creates implementation graph.

Agent 1:
Terrain + foliage.

Agent 2:
Player controller.

Agent 3:
Lighting + materials.

Agent 4:
Gameplay/UI.

Verifier:
Runs project and tests integration.

But coordination must happen through a central orchestrator so agents do not overwrite each other's work.

---

# 134. AI TOOL DISCOVERY

Do not hardcode all tools into every AI prompt.

Provide tool schema discovery.

Example:

```text
list_engine_tools("physics")
```

returns relevant engine commands.

This keeps context manageable as engine capabilities grow.

---

# 135. AI SAFETY FOR DESTRUCTIVE OPERATIONS

AI should understand when operations are destructive.

Examples:

* delete scene
* remove 500 entities
* replace material globally
* modify build settings
* delete assets

Group actions into reversible transactions wherever possible.

---

# 136. EDITOR SHORTCUTS

Implement configurable shortcuts.

Common patterns:

```text
W = Move
E = Rotate
R = Scale
F = Focus Selected
Delete = Delete
Ctrl/Cmd + Z = Undo
Ctrl/Cmd + Shift + Z = Redo
Ctrl/Cmd + S = Save
```

Allow remapping.

---

# 137. HISTORY

Maintain editor history including:

* selections
* opened assets
* commands
* changes
* AI operations

Useful for debugging and undo.

---

# 138. DESIGN QUALITY

Bhippi Creator should not visually clone Unreal Engine.

Keep Bhippi's identity.

The engine UI should feel:

* dark
* extremely clean
* professional
* dense when necessary
* readable
* modern
* fast
* non-gimmicky

Use subtle separation and hierarchy.

Avoid:

* excessive gradients
* random neon
* glass everywhere
* giant cards
* oversized whitespace
* unnecessary animations
* fake futuristic elements

The engine must feel like a serious development environment.

---

# 139. MICROINTERACTIONS

Animations should communicate state rather than distract.

Examples:

* smooth tab change
* subtle panel resize
* object selection highlight
* compilation spinner
* agent state
* import progress
* play mode indicator
* warnings

Avoid animated UI that slows professional workflows.

---

# 140. PERFORMANCE TARGET

The editor should remain responsive with complex projects.

Track metrics such as:

* editor FPS
* game FPS
* frame time
* scene entity count
* draw calls
* triangles
* memory
* shader compilation
* asset import time

Do not sacrifice architecture for unrealistic benchmark promises.

---

# 141. DOCUMENT EVERYTHING

Create developer documentation:

```text
/docs
    architecture.md
    ecs.md
    rendering.md
    materials.md
    physics.md
    scripting.md
    animation.md
    ai-engine-bridge.md
    serialization.md
    plugins.md
    build-system.md
```

Create diagrams showing important data flows.

---

# 142. ARCHITECTURE DEPENDENCY RULE

Prefer dependencies flowing conceptually like:

```text
Platform
   ↓
Core
   ↓
Runtime Systems
   ↓
Engine API
   ↓
Editor / AI Bridge
```

The Core must never depend on React/editor UI.

Physics must not depend on Inspector.

Renderer must not depend on chat.

Chat must communicate through engine interfaces.

---

# 143. START BY AUDITING THE EXISTING PROJECT

Before implementing anything:

Perform a full codebase audit.

Determine:

* current framework
* rendering technology
* existing viewport
* existing scene implementation
* current AI/chat architecture
* existing file system
* existing database
* state management
* project representation
* asset representation
* desktop/web architecture
* existing command infrastructure

Then create:

```text
ENGINE_AUDIT.md
ENGINE_ARCHITECTURE.md
ENGINE_IMPLEMENTATION_PLAN.md
ENGINE_PROGRESS.md
```

---

# 144. ENGINE_PROGRESS.md

Maintain progress continuously.

Example:

```text
[COMPLETE] ECS
[COMPLETE] Transform
[COMPLETE] Scene Serialization

[IN PROGRESS] PBR Renderer
    [x] Mesh pass
    [x] Texture loading
    [ ] Shadows
    [ ] Environment lighting

[PLANNED] Physics
```

Do not mark something complete unless it works.

---

# 145. ARCHITECTURAL DECISION RECORDS

For major decisions record:

```text
/docs/adr/
```

Example:

```text
ADR-001-render-api.md
ADR-002-ecs-storage.md
ADR-003-physics-backend.md
```

Include:

* problem
* options
* chosen approach
* reason
* tradeoffs

This prevents future coding agents from repeatedly reversing architecture.

---

# 146. BUILD REAL VERTICAL SLICES

After creating foundations, make one real test project.

For example:

## Engine Test World

Contains:

* terrain
* sky
* sun
* PBR objects
* player
* third-person camera
* physics cubes
* enemy
* animation
* particles
* sound
* UI
* post processing

This project acts as a constant integration test.

---

# 147. DO NOT HIDE TECHNICAL DEBT

If a subsystem is temporarily limited, record the limitation.

Example:

```text
LIMITATION:
Point-light shadow cubemaps currently limited to four shadow-casting lights.
```

This is preferable to pretending everything works.

---

# 148. ERROR-FIRST ENGINE DEVELOPMENT

Every subsystem must expose useful diagnostics.

Bad:

```text
Failed.
```

Good:

```text
ShaderCompilationError
Material: M_Water
Node: NormalBlend
Backend: WebGPU
Message: Expected vec3, received float.
```

The AI depends on high-quality errors.

---

# 149. AI OBSERVABILITY

AI should be able to query:

```text
What changed?
What failed?
What is selected?
What is running?
What is compiling?
What warnings exist?
What entities changed?
What assets changed?
What scripts changed?
What is the current FPS?
```

Expose these as structured APIs.

---

# 150. FINAL PRODUCT EXPECTATION

The result should not be described as:

"a 3D feature inside Bhippi."

It should become:

**Bhippi Engine — the AI-native game engine integrated directly into Bhippi Creator.**

A user should eventually be able to create:

* simple 2D games
* 3D games
* mobile games
* PC games
* interactive experiences
* visualizations
* cinematics
* simulations
* multiplayer projects

inside one integrated environment.

The human and the AI should share the same project, the same scene, the same engine state, and the same editing commands.

---

# ABSOLUTE DEVELOPMENT PRINCIPLE

Whenever there is a choice between:

```text
fake breadth
```

and:

```text
real working depth
```

choose real working depth.

Build the engine foundations correctly.

Do not create hundreds of fake Unreal-like options just for appearance.

Every visible control should eventually correspond to a real system.

Every real system should expose a documented API.

Every important API should be available to the editor.

Every relevant editor command should be available to the AI.

Every important operation should be observable and reversible.

The final goal is not to create something that **looks like a game engine**.

The final goal is to create something that **is a game engine**.

make this engine good and attach this to to chat arctuure so the ai proeprly understands and can edit and look and work inisde the engine on its own
, make sure this is only the engine plan , make it checkable so whhen a thing or task is completed u check it and writes it like this is done and have created these fiels so it understands 