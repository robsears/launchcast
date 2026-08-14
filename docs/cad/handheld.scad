SHOW_GHOSTS = false;
SHOW_LID    = true;
SHOW_BOX    = false;
$fn = 48;

// ---- Footprints ----
PCB_RADIUS    = 2.5;    // Corner radius of the PCB
PCB_THICKNESS = 2.0;    // Thickness of PCB
PCB_HOLE_D    = 2.5;    // Diameter of pass-thru holes on PCB

// ---- Feather ----
FEATHER_Z      = 50.8;  // 2.00"
FEATHER_Y      = 22.9;  // 0.90"
FEATHER_HOLE_Z = 45.7;  // 1.80"
FEATHER_HOLE_Y = 17.8;  // 0.70"

// ---- GPS ----
GPS_Z      = 25.4;      // 1.00"
GPS_Y      = 25.4;      // 1.00"
GPS_HOLE_Z = 20.3;      // 0.80"
GPS_HOLE_Y = 20.3;      // 0.80"

// ---- Cutouts --
USB_CHARGER_D = 21.0;
ANTENNA_D     = 6.5;
ONOFF_X       = 19.5;
ONOFF_Z       = 13.5;

// ---- Battery ----
BATTERY_X = 61.0;
BATTERY_Y = 57.5;
BATTERY_Z = 7.75;
BATTERY_HOLDER_THICKNESS = 2.0;

// ---- Button Pad ----
BUTTON_PAD_X = 80.0;
BUTTON_PAD_Y = 20.0;
BUTTON_PAD_Z = PCB_THICKNESS;
BUTTON_PAD_POST_H = 18.1; //OAL of the button
BUTTON_PAD_HOLE_X = 76;
BUTTON_PAD_HOLE_Y = 16;
BUTTON_PAD_HOLE_D = 2.0;
BUTTON_BASE_X     = 12.0;
BUTTON_BASE_Y     = 12.0;
BUTTON_BASE_Z     = 6.4;
BUTTON_PLUNGER_X  = 3.8;
BUTTON_PLUNGER_Y  = 3.8;
BUTTON_PLUNGER_Z  = 3.9;
BUTTON_BUTTON_BASE_H = 1.4;
BUTTON_BUTTON_BASE_D = 12.8;
BUTTON_BUTTON_BODY_H = 4.4;
BUTTON_BUTTON_BODY_D = 12.0;

// ---- Inserts (M2.5 x D3.0 x L4.0) ----
INSERT_BORE_D          = 3.2; // DIA of pilot hole
INSERT_DEPTH           = 5.0;
INSERT_WALL_THICKNESS  = 2.0;
INSERT_WALL_THICKNESS_MIN = 1.6;
INSERT_FLOOR_THICKNESS = 1.0;

POST_OD_HEAVY = ((INSERT_BORE_D / 2) + INSERT_WALL_THICKNESS)*2;  // Feather, battery bar
POST_OD_LIGHT = ((INSERT_BORE_D / 2) + INSERT_WALL_THICKNESS)*2;  // GPS, IMU, BMP
POST_H_STD    = max(4.5, INSERT_DEPTH);
POST_H_GPS    = max(5.6, INSERT_DEPTH);;  // clears 4.12mm coin holder + 1.5mm

// -- Screen dimensions
SCREEN_PCB_X = 63.5;              // 2.50"
SCREEN_PCB_Y = 55.88;             // 2.20"
SCREEN_PCB_HOLES_X = 58.42;       // 2.30"
SCREEN_PCB_HOLES_Y = 50.8;        // 2.00"
SCREEN_PCB_HOLE_OFFSET = 2.54;     //
SCREEN_DISPLAY_X = SCREEN_PCB_X;  // TODO: measure?
SCREEN_DISPLAY_Y = 43.4;          // Caliper measurement
SCREEN_DISPLAY_Y_OFFSET = 6.75;   // Caliper measurement
SCREEN_DISPLAY_Z = PCB_THICKNESS; // TODO: measure? Does it matter?
SCREEN_DISPLAY_GRADE = 5;

// Unit dimensions
HANDHELD_THICKNESS = 1.5;
HANDHELD_TOP_X     = 150;
HANDHELD_TOP_Y     = 100;
HANDHELD_TOP_Z     = SCREEN_DISPLAY_Z;
HANDHELD_OVERALL_Z = 37;
HANDHELD_FILLET    = 5.0;
HANDHELD_INSERT_THICKNESS = INSERT_DEPTH + INSERT_FLOOR_THICKNESS;
HANDHELD_DISPLAY_BORDER = (INSERT_BORE_D + 2 * INSERT_WALL_THICKNESS) * 2;
HANDHELD_DISPLAY_X = (HANDHELD_TOP_X - SCREEN_DISPLAY_X)/2;
HANDHELD_DISPLAY_Y = (HANDHELD_TOP_Y - SCREEN_DISPLAY_Y - 20);

HEAT_SET_INSERT_D     = 6.3;
HEAT_SET_INSERT_WALL  = 2.8;
HEAT_SET_INSERT_DEPTH = 8.0;

echo(16.5 + BATTERY_Z + 2*BATTERY_HOLDER_THICKNESS + 2.5 * HANDHELD_THICKNESS);

module rounded_rect(x, y, z, r, $fn=32) {
  translate([x/2, y/2, 0]) {
    linear_extrude(height=z)
    minkowski() {
      square([x - 2*r, y - 2*r], center=true);
      circle(r=r);
    }    
  }
}

module ghost(y, z, col) {
  color(col,0.35)
    rounded_rect(y, z, PCB_THICKNESS, PCB_RADIUS);
}

module screen_ghost() {
  pcb_hole_x = (SCREEN_PCB_X - SCREEN_PCB_HOLES_X)/2;
  pcb_hole_y = (SCREEN_PCB_Y - SCREEN_PCB_HOLES_Y)/2;
  union() {
    difference() {
      color("YellowGreen",1) {
        cube([SCREEN_PCB_X, SCREEN_PCB_Y, PCB_THICKNESS]);
      }
      translate ([pcb_hole_x, pcb_hole_y, 0]) { cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2); }
      translate ([pcb_hole_x + SCREEN_PCB_HOLES_X, pcb_hole_y, 0]) { cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2); }
      translate ([pcb_hole_x, pcb_hole_y + SCREEN_PCB_HOLES_Y, 0]) { cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2); }
      translate ([pcb_hole_x + SCREEN_PCB_HOLES_X, pcb_hole_y + SCREEN_PCB_HOLES_Y, 0]) { cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2); }
    }
    color("Black",0.35)
      translate([0, SCREEN_DISPLAY_Y_OFFSET, PCB_THICKNESS]) {
        cube([SCREEN_DISPLAY_X, SCREEN_DISPLAY_Y, PCB_THICKNESS]);
      }
  }
}

// adapted from docs:
// https://en.wikibooks.org/wiki/OpenSCAD_User_Manual/Primitive_Solids#polyhedron
module screen_cutout() {
  CubePoints = [
    [  0,                                      0,                                        0 ],                          //0
    [ SCREEN_DISPLAY_X,                        0,                                        0 ],                          //1
    [ SCREEN_DISPLAY_X,                        SCREEN_DISPLAY_Y,                         0 ],                          //2
    [ 0,                                       SCREEN_DISPLAY_Y,                         0 ],                          //3
    [ -SCREEN_DISPLAY_GRADE,                   -SCREEN_DISPLAY_GRADE,                    HANDHELD_INSERT_THICKNESS ],  //4
    [ SCREEN_DISPLAY_X + SCREEN_DISPLAY_GRADE, -SCREEN_DISPLAY_GRADE,                    HANDHELD_INSERT_THICKNESS ],  //5
    [ SCREEN_DISPLAY_X + SCREEN_DISPLAY_GRADE, SCREEN_DISPLAY_Y + SCREEN_DISPLAY_GRADE,  HANDHELD_INSERT_THICKNESS ],  //6
    [ -SCREEN_DISPLAY_GRADE,                   SCREEN_DISPLAY_Y + SCREEN_DISPLAY_GRADE,  HANDHELD_INSERT_THICKNESS  ]  //7
  ];
    
  CubeFaces = [
    [0,1,2,3],  // bottom
    [4,5,1,0],  // front
    [7,6,5,4],  // top
    [5,6,2,1],  // right
    [6,7,3,2],  // back
    [7,4,0,3]]; // left
    
  polyhedron( CubePoints, CubeFaces );
}

module post(x, y, h, od) {
  rotate([0, -90, 0]) {
    translate([x, y, 0]) {
      difference() {
        cylinder(h = h, r1 = od/2, r2 = 1.3 * (od/2));
        translate([0, 0, INSERT_DEPTH-h]) {
          cylinder(h = INSERT_DEPTH, r = INSERT_BORE_D/2);
        }
      }
      if (SHOW_GHOSTS) {
        translate([0, 0, h - INSERT_DEPTH]) {
          color("Magenta",0.35)
            cylinder(h = INSERT_DEPTH, r = INSERT_BORE_D/2);
        }
      }
    }
  }
}

module post_feather(x, y) {
  post(x, y, POST_H_STD, POST_OD_HEAVY);
}

module post_gps(x, y) {
  post(x, y, POST_H_GPS, POST_OD_LIGHT);
}

// --- Feather Mounts + Ghost
// Posts flush with X=0, YZ at midpoint
module feather_posts() {
  rotate([90, -90, 0]) {
    translate([POST_H_STD, 0, 0]) {
      if (SHOW_GHOSTS) {
        translate([0, -FEATHER_Y/2, -FEATHER_Z/2]) {rotate([90, 0, 90]) {feather_ghost();}}
      }
      translate([0, -FEATHER_HOLE_Y/2, FEATHER_HOLE_Z/2]) {
        post_feather(0, 0);
        post_feather(0, FEATHER_HOLE_Y);
        post_feather(-FEATHER_HOLE_Z, 0);
        post_feather(-FEATHER_HOLE_Z, FEATHER_HOLE_Y);
      }
    }
  }
}

module feather_ghost() {
  difference() {
    ghost(FEATHER_Y, FEATHER_Z, "GreenYellow");
    translate([0, -FEATHER_HOLE_Y/2, -FEATHER_HOLE_Z/2]) {
      rotate([0, 90, 0]) {
        translate([0, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([0, FEATHER_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-FEATHER_HOLE_Z, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-FEATHER_HOLE_Z, FEATHER_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
      }
    }
  }
}

// --- GPS Mounts + Ghost
// Posts flush with X=0, YZ at midpoint
module gps_posts() {
  rotate([90, -90, 0]) {
    translate([POST_H_GPS, 0, 0]) {
      if (SHOW_GHOSTS) {
        translate([0, -GPS_Y/2, -GPS_Z/2]) {rotate([90, 0, 90]) {gps_ghost();}}
      }
      translate([0, -GPS_HOLE_Y/2, GPS_HOLE_Z/2]) {
        post_gps(0, 0);
        post_gps(0, GPS_HOLE_Y);
        post_gps(-GPS_HOLE_Z, 0);
        post_gps(-GPS_HOLE_Z, GPS_HOLE_Y);
      }
    }
  }
}

module gps_ghost() {
  difference() {
    ghost(GPS_Y, GPS_Z, "DeepSkyBlue");
    translate([0, -GPS_HOLE_Y/2, -GPS_HOLE_Z/2]) {
      rotate([0, 90, 0]) {
        translate([0, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([0, GPS_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-GPS_HOLE_Z, 0, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
        translate([-GPS_HOLE_Z, GPS_HOLE_Y, 0]) {cylinder(h = PCB_THICKNESS, r = PCB_HOLE_D/2);}
      }
    }
  }
}

module button() {
  union() {
    cube([BUTTON_BASE_X, BUTTON_BASE_Y, BUTTON_BASE_Z]);
    translate([(BUTTON_BASE_X - BUTTON_PLUNGER_X)/2, (BUTTON_BASE_Y - BUTTON_PLUNGER_Y)/2, BUTTON_BASE_Z]) {
      cube([BUTTON_PLUNGER_X, BUTTON_PLUNGER_Y, BUTTON_PLUNGER_Z]);
    }
    translate([
      BUTTON_BASE_X/2, BUTTON_BASE_Y/2, BUTTON_BASE_Z + BUTTON_PLUNGER_Z
    ]) { cylinder(h = BUTTON_BUTTON_BASE_H, r = BUTTON_BUTTON_BASE_D/2); }
    translate([
      BUTTON_BASE_X/2, BUTTON_BASE_Y/2, BUTTON_BASE_Z + BUTTON_PLUNGER_Z + BUTTON_BUTTON_BASE_H
    ]) { cylinder(h = BUTTON_BUTTON_BODY_H, r = BUTTON_BUTTON_BODY_D/2); }
  }
}


module button_pad() {
  oal = PCB_THICKNESS + BUTTON_BASE_Z + BUTTON_PLUNGER_Z + BUTTON_BUTTON_BASE_H + BUTTON_BUTTON_BODY_H;
  height = oal - HANDHELD_THICKNESS - 2.2;
  translate([0, BUTTON_PAD_Y, height]) {
    rotate([180, 0, 0]) {
      if (SHOW_GHOSTS) {
        color("Blue", 0.35) {
          difference() { // PCB with holes
            pcb_hole_x = (BUTTON_PAD_X - BUTTON_PAD_HOLE_X)/2;
            pcb_hole_y = (BUTTON_PAD_Y - BUTTON_PAD_HOLE_Y)/2;
            rounded_rect(BUTTON_PAD_X, BUTTON_PAD_Y, BUTTON_PAD_Z, PCB_RADIUS);
            translate([pcb_hole_x, pcb_hole_y, -PCB_THICKNESS]) {cylinder(h = 3 * BUTTON_PAD_Z, r = BUTTON_PAD_HOLE_D/2);}
            translate([pcb_hole_x, pcb_hole_y + BUTTON_PAD_HOLE_Y, -PCB_THICKNESS]) {cylinder(h = 3 * BUTTON_PAD_Z, r = BUTTON_PAD_HOLE_D/2);}
            translate([pcb_hole_x + BUTTON_PAD_HOLE_X, pcb_hole_y, -PCB_THICKNESS]) {cylinder(h = 3 * BUTTON_PAD_Z, r = BUTTON_PAD_HOLE_D/2);}
            translate([pcb_hole_x + BUTTON_PAD_HOLE_X, pcb_hole_y + BUTTON_PAD_HOLE_Y, -PCB_THICKNESS]) {cylinder(h = 3 * BUTTON_PAD_Z, r = BUTTON_PAD_HOLE_D/2);}
          }
        }
      }
      midpoint = (BUTTON_PAD_Y - BUTTON_BASE_Y)/2;
      translate([4.9, midpoint, PCB_THICKNESS]) { button(); }
      translate([(4.9+BUTTON_BASE_X+18.66), midpoint, PCB_THICKNESS]) { button(); }
      translate([(4.9+18.66+18.6 + 2*BUTTON_BASE_X), midpoint, PCB_THICKNESS]) { button(); }
    }
  }
}

module button_pad_posts() {
  post_h = BUTTON_PAD_POST_H - BUTTON_BASE_Z - PCB_THICKNESS - HANDHELD_THICKNESS - 2.2;
  echo(post_h)
  echo(post_h + BUTTON_BASE_Z)
  translate([(BUTTON_PAD_X - BUTTON_PAD_HOLE_X)/2, (BUTTON_PAD_Y - BUTTON_PAD_HOLE_Y)/2, 0]) {
    difference() { //posts
      union() {
        translate([0,                 0,                  0]) { cylinder(h = post_h, r = (INSERT_BORE_D + 2*INSERT_WALL_THICKNESS)/2); }
        translate([0,                 BUTTON_PAD_HOLE_Y,  0]) { cylinder(h = post_h, r = (INSERT_BORE_D + 2*INSERT_WALL_THICKNESS)/2); }
        translate([BUTTON_PAD_HOLE_X, 0,                  0]) { cylinder(h = post_h, r = (INSERT_BORE_D + 2*INSERT_WALL_THICKNESS_MIN)/2); }
        translate([BUTTON_PAD_HOLE_X, BUTTON_PAD_HOLE_Y,  0]) { cylinder(h = post_h, r = (INSERT_BORE_D + 2*INSERT_WALL_THICKNESS_MIN)/2); }
      }
      union() { //bores
        translate([0,                 0,                 post_h - INSERT_DEPTH]) { cylinder(h = INSERT_DEPTH, r = (INSERT_BORE_D)/2); }
        translate([0,                 BUTTON_PAD_HOLE_Y, post_h - INSERT_DEPTH]) { cylinder(h = INSERT_DEPTH, r = (INSERT_BORE_D)/2); }
        translate([BUTTON_PAD_HOLE_X, 0,                 post_h - INSERT_DEPTH]) { cylinder(h = INSERT_DEPTH, r = (INSERT_BORE_D)/2); }
        translate([BUTTON_PAD_HOLE_X, BUTTON_PAD_HOLE_Y, post_h - INSERT_DEPTH]) { cylinder(h = INSERT_DEPTH, r = (INSERT_BORE_D)/2); }
      }
    }
  }
}

module control_box() {
  difference() { // Hollow box with beveled screen cutout
    union() { // Hollow box w/display border
      difference() { // Hollow box
        rounded_rect(HANDHELD_TOP_X, HANDHELD_TOP_Y, HANDHELD_OVERALL_Z, HANDHELD_FILLET);
        translate([(HANDHELD_THICKNESS), (HANDHELD_THICKNESS), HANDHELD_THICKNESS]) {
          rounded_rect(HANDHELD_TOP_X - 2*HANDHELD_THICKNESS, HANDHELD_TOP_Y-2*HANDHELD_THICKNESS, HANDHELD_OVERALL_Z-HANDHELD_THICKNESS, HANDHELD_FILLET);
        }
      }
      difference() { // Display border with heat insert cutouts
        translate([ // Display mounting border
          HANDHELD_DISPLAY_X - HANDHELD_DISPLAY_BORDER/2,
          HANDHELD_DISPLAY_Y + SCREEN_DISPLAY_Y_OFFSET - HANDHELD_DISPLAY_BORDER/2,
          HANDHELD_THICKNESS]) {
          cube([
            SCREEN_DISPLAY_X + HANDHELD_DISPLAY_BORDER,
            SCREEN_DISPLAY_Y + HANDHELD_DISPLAY_BORDER,
            HANDHELD_INSERT_THICKNESS]);
        }
        display_post_x = HANDHELD_DISPLAY_X + SCREEN_PCB_HOLE_OFFSET;
        display_post_y = HANDHELD_DISPLAY_Y + SCREEN_PCB_HOLE_OFFSET;
        display_post_d = INSERT_DEPTH;
        translate([display_post_x, display_post_y+SCREEN_PCB_HOLES_Y, display_post_d + 2.5 - INSERT_DEPTH]) { cylinder(r = INSERT_BORE_D/2, h = INSERT_DEPTH); }
        translate([display_post_x, display_post_y, display_post_d + 2.5 - INSERT_DEPTH]) { cylinder(r = INSERT_BORE_D/2, h = INSERT_DEPTH); }
        translate([display_post_x+SCREEN_PCB_HOLES_X, display_post_y+SCREEN_PCB_HOLES_Y, display_post_d + 2.5 - INSERT_DEPTH]) { cylinder(r = INSERT_BORE_D/2, h = INSERT_DEPTH); }
        translate([display_post_x+SCREEN_PCB_HOLES_X, display_post_y, display_post_d + 2.5 - INSERT_DEPTH]) { cylinder(r = INSERT_BORE_D/2, h = INSERT_DEPTH); }
      }
      translate([(HANDHELD_TOP_X - BUTTON_PAD_X)/2-1, 12, HANDHELD_THICKNESS]) { button_pad_posts(); }
    } 
    translate([ // Beveled display window cutout
      HANDHELD_DISPLAY_X,
      HANDHELD_DISPLAY_Y + SCREEN_DISPLAY_Y_OFFSET,
      (HANDHELD_INSERT_THICKNESS - HANDHELD_TOP_Z)-SCREEN_DISPLAY_Z]
    ) {
      translate([0, HANDHELD_DISPLAY_Y + SCREEN_DISPLAY_Y_OFFSET, 0]) {rotate([180, 0, 0]) {screen_cutout();}}
      cube([
        SCREEN_DISPLAY_X,
        SCREEN_DISPLAY_Y,
        HANDHELD_INSERT_THICKNESS + 2 * HANDHELD_TOP_Z
      ]);
    }
    union() { // misc cutouts
      // charging port
      translate([HANDHELD_TOP_X-2*HANDHELD_THICKNESS, HANDHELD_TOP_Y/2 - 17, HANDHELD_OVERALL_Z/2]) { rotate([90, 0, 90]) { cylinder(r = USB_CHARGER_D/2, h = 3*HANDHELD_THICKNESS); }}
      
      // antenna
      translate([FEATHER_Y/2 + 2*HANDHELD_THICKNESS + 7, HANDHELD_TOP_Y + HANDHELD_THICKNESS, HANDHELD_OVERALL_Z/2]) { rotate([90, 0, 0]) { cylinder(r = ANTENNA_D/2, h = 3*HANDHELD_THICKNESS); }}
      
      // onoff switch
      translate([HANDHELD_TOP_X - 2*ONOFF_X+5, HANDHELD_TOP_Y - 1.5*HANDHELD_THICKNESS, (HANDHELD_OVERALL_Z - ONOFF_Z)/2]) { rotate([0, 0, 0]) { cube([ONOFF_X, 3*HANDHELD_THICKNESS, ONOFF_Z]); }}
      
      // buttons
      translate([(HANDHELD_TOP_X - BUTTON_PAD_X)/2-1, 12, HANDHELD_THICKNESS]) { rotate([0, 0, 0]) { button_pad(); }}
      
      // "Menu"
      translate ([112, 10, 0.5]) { rotate([0, 180, 0]) { text_extrude("Menu", font = "DejaVu Sans", letter_size = 5); } }
      
      // "Arm/Disarm"
      translate ([75, 10, 0.5]) { rotate([0, 180, 0]) { text_extrude("Arm/Disarm", font = "DejaVu Sans", letter_size = 5); } }
      
      // Chirp
      translate ([40, 10, 0.5]) { rotate([0, 180, 0]) { text_extrude("Chirp", font = "DejaVu Sans", letter_size = 5); } }
    }
  }
  difference() { // Corner posts w/ heat insert cutouts
    union() { // Corner posts
      posts = HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL;
      height = HANDHELD_OVERALL_Z - 2*HANDHELD_THICKNESS;
      trans_h = HANDHELD_OVERALL_Z - height - 2*HANDHELD_THICKNESS;
      translate([0, 0, trans_h]) {
        rounded_rect(posts, posts, height, HANDHELD_FILLET);}
      translate([HANDHELD_TOP_X - posts, 0, trans_h]) {
        rounded_rect(posts, posts, height, HANDHELD_FILLET);}
      translate([0, HANDHELD_TOP_Y - posts, trans_h]) {
        rounded_rect(posts, posts, height, HANDHELD_FILLET);}
      translate([HANDHELD_TOP_X - posts, HANDHELD_TOP_Y - posts, trans_h]) {
        rounded_rect(posts, posts, height, HANDHELD_FILLET);}
    }
    union() { // heat insert cutouts
      height = HEAT_SET_INSERT_DEPTH;
      translate([
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2,
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2,
        HANDHELD_OVERALL_Z - height - HANDHELD_THICKNESS
      ]) {
        cylinder(r = HEAT_SET_INSERT_D/2, h = HEAT_SET_INSERT_DEPTH);}
      translate([
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2,
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2 + HANDHELD_TOP_Y - HEAT_SET_INSERT_D - 2 * HEAT_SET_INSERT_WALL,
        HANDHELD_OVERALL_Z - height - HANDHELD_THICKNESS
      ]) {
        cylinder(r = HEAT_SET_INSERT_D/2, h = HEAT_SET_INSERT_DEPTH);}
      translate([
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2 + HANDHELD_TOP_X - HEAT_SET_INSERT_D - 2 * HEAT_SET_INSERT_WALL,
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2 + HANDHELD_TOP_Y - HEAT_SET_INSERT_D - 2 * HEAT_SET_INSERT_WALL,
        HANDHELD_OVERALL_Z - height - HANDHELD_THICKNESS
      ]) {
        cylinder(r = HEAT_SET_INSERT_D/2, h = HEAT_SET_INSERT_DEPTH);}
      translate([
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2 + HANDHELD_TOP_X - HEAT_SET_INSERT_D - 2 * HEAT_SET_INSERT_WALL,
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2,
        HANDHELD_OVERALL_Z - height - HANDHELD_THICKNESS
      ]) {
        cylinder(r = HEAT_SET_INSERT_D/2, h = HEAT_SET_INSERT_DEPTH);}
    }
  }
}

module battery_holder() {
  fillet = 7;
  translate([0, 0, 0]) {
    difference() {
      cube([BATTERY_X, BATTERY_Y + 2*BATTERY_HOLDER_THICKNESS, BATTERY_Z + 2*BATTERY_HOLDER_THICKNESS]);
      translate([BATTERY_X - BATTERY_HOLDER_THICKNESS, BATTERY_HOLDER_THICKNESS, BATTERY_HOLDER_THICKNESS]) {
        union() { //battery cutout
          rotate([0, -90, 0]) {rounded_rect(BATTERY_Z, BATTERY_Y, BATTERY_X, PCB_RADIUS);}
          translate ([0, fillet/2, 5]) {rotate([0, -90, 0]) {rounded_rect(BATTERY_Z, BATTERY_Y - fillet, BATTERY_X, PCB_RADIUS);}}
        }
      }
    }
  }
}

module control_box_with_posts() {
  union() {
    control_box();
    
    // Feather posts
    translate([FEATHER_Y/2 + HANDHELD_THICKNESS + 7, HANDHELD_TOP_Y/2 + 10, HANDHELD_THICKNESS]) { feather_posts(); }
    
    // GPS posts
    translate([HANDHELD_TOP_X - GPS_Y/2 - HANDHELD_THICKNESS - 5, HANDHELD_TOP_Y/2 + 17, HANDHELD_THICKNESS]) { gps_posts(); }
  }
}

module m5_screw() {
  l  = 6.0;
  k  = 2.8;
  d  = 5.0;
  dk = 10.0;
  cylinder(h = l, r = d/2);
  translate([0, 0, l]) {cylinder(h = k, r1 = d/2, r2 = dk/2);}
  translate([0, 0, l+k]) {cylinder(h = k, r = dk/2);}
}

module text_extrude(string, font = "DejaVu Sans:style=Bold", letter_size = 10) {
  height = 5;
  linear_extrude(height)
    text(string, size = letter_size, font = font, halign = "center", valign = "center", $fn = 64);
}

module back_cover() {
  screw_height = 8.8; // 6.0+2.8
  translate([0, 0, 0]) {
    difference() {
      union() {
        translate([0, 0, HANDHELD_THICKNESS]) {rounded_rect(HANDHELD_TOP_X, HANDHELD_TOP_Y, HANDHELD_THICKNESS, HANDHELD_FILLET);}
        translate([
          HANDHELD_THICKNESS,
          HANDHELD_THICKNESS,
          0
        ]) {rounded_rect(HANDHELD_TOP_X - 2.5*HANDHELD_THICKNESS, HANDHELD_TOP_Y - 2.5*HANDHELD_THICKNESS, 1.5 * HANDHELD_THICKNESS, HANDHELD_FILLET);}
      }
      translate([
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2,
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2,
        -screw_height + 2*HANDHELD_THICKNESS
      ]) { m5_screw(); }
      translate([
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2,
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2 + HANDHELD_TOP_Y - HEAT_SET_INSERT_D - 2 * HEAT_SET_INSERT_WALL,
        -screw_height + 2*HANDHELD_THICKNESS
      ]) { m5_screw(); }
      translate([
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2 + HANDHELD_TOP_X - HEAT_SET_INSERT_D - 2 * HEAT_SET_INSERT_WALL,
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2 + HANDHELD_TOP_Y - HEAT_SET_INSERT_D - 2 * HEAT_SET_INSERT_WALL,
        -screw_height + 2*HANDHELD_THICKNESS
      ]) { m5_screw(); }
      translate([
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2 + HANDHELD_TOP_X - HEAT_SET_INSERT_D - 2 * HEAT_SET_INSERT_WALL,
        (HEAT_SET_INSERT_D + 2 * HEAT_SET_INSERT_WALL)/2,
        -screw_height + 2*HANDHELD_THICKNESS
      ]) { m5_screw(); }
      translate([HANDHELD_TOP_X/2, HANDHELD_TOP_Y/2, 2*HANDHELD_THICKNESS - 0.5]) {
//        text_extrude("LaunchCast");
      }
    }
  }
}

module back_cover_with_battery() {
  union() {
    translate ([-HANDHELD_TOP_X - 10, HANDHELD_TOP_Y, 2*HANDHELD_THICKNESS]) { rotate([180, 0, 0]) { back_cover();} }
    translate([BATTERY_X/2-(2*HANDHELD_TOP_X - 10)/2-15, (HANDHELD_TOP_Y-BATTERY_Y)/2-10, 2*HANDHELD_THICKNESS]) {
      battery_holder();
    }
  }
}

if (SHOW_BOX) { translate([10, 0, 0]) { control_box_with_posts(); } }
if (SHOW_LID) { translate([0, -10, 0]) {rotate([0, 0, 180]) { back_cover_with_battery(); }}}
